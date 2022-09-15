use chatsounds::Chatsounds;
use classicube_helpers::{
    entities::{Entities, ENTITY_SELF_ID},
    tab_list::{remove_color, TabList},
};
use classicube_sys::{MsgType, MsgType_MSG_TYPE_NORMAL, Server, Vec3, WindowInfo};

use super::{entity_emitter::EntityEmitter, random, send_entity::SendEntity};
use crate::{
    helpers::is_continuation_message,
    modules::{
        chatsounds::random::get_rng,
        event_handler::{IncomingEvent, IncomingEventListener},
        FutureShared, FuturesModule, SyncShared, ThreadShared,
    },
    printer::print,
};

pub struct ChatsoundsEventListener {
    chatsounds: FutureShared<Option<Chatsounds>>,
    entity_emitters: ThreadShared<Vec<EntityEmitter>>,
    chat_last: Option<String>,
    tab_list: SyncShared<TabList>,
    entities: SyncShared<Entities>,
}

impl ChatsoundsEventListener {
    pub fn new(
        tab_list: SyncShared<TabList>,
        entities: SyncShared<Entities>,
        chatsounds: FutureShared<Option<Chatsounds>>,
    ) -> Self {
        Self {
            chatsounds,
            entity_emitters: Default::default(),
            chat_last: None,
            tab_list,
            entities,
        }
    }

    fn find_player_from_message(&mut self, mut full_msg: String) -> Option<(u8, String, String)> {
        if unsafe { Server.IsSinglePlayer } != 0 {
            // in singleplayer there is no tab list, even self id infos are null

            return Some((ENTITY_SELF_ID, String::new(), full_msg));
        }

        if let Some(continuation) = is_continuation_message(&full_msg) {
            if let Some(chat_last) = &self.chat_last {
                // we're a continue message
                full_msg = continuation.to_string();

                // most likely there's a space
                // the server trims the first line :(
                full_msg = format!("{} {}", chat_last, full_msg);
                self.chat_last = Some(full_msg.clone());
            }
        } else {
            // normal message start
            self.chat_last = Some(full_msg.clone());
        }

        // &]SpiralP: &faaa
        // let full_msg = full_msg.into();

        // nickname_resolver_handle_message(full_msg.to_string());

        // find colon from the left
        let opt = full_msg
            .find(": ")
            .and_then(|pos| if pos > 4 { Some(pos) } else { None });
        if let Some(pos) = opt {
            // &]SpiralP
            let left = &full_msg[..pos]; // left without colon
                                         // &faaa
            let right = &full_msg[(pos + 2)..]; // right without colon

            // TODO title is [ ] before nick, team is < > before nick, also there are rank
            // symbols? &f┬ &f♂&6 Goodly: &fhi

            let full_nick = left.to_string();
            let said_text = right.to_string();

            // lookup entity id from nick_name by using TabList
            self.tab_list
                .borrow_mut()
                .find_entry_by_nick_name(&full_nick)
                .and_then(|entry| {
                    entry
                        .upgrade()
                        .map(|entry| (entry.get_id(), entry.get_real_name(), said_text))
                })
        } else {
            None
        }
    }

    // run this sync so that chat_last comes in order
    fn handle_chat_received(&mut self, full_msg: String, msg_type: MsgType) {
        if msg_type != MsgType_MSG_TYPE_NORMAL {
            return;
        }

        let focused = unsafe { WindowInfo.Focused } != 0;
        if !focused {
            return;
        }

        if let Some((id, real_name, said_text)) = self.find_player_from_message(full_msg) {
            random::update_chat_count(&real_name);

            let entities = self.entities.borrow_mut();
            if let Some(entity) = entities.get(id).and_then(|e| e.upgrade()) {
                // if entity is in our map
                if let Some(self_entity) = entities.get(ENTITY_SELF_ID).and_then(|e| e.upgrade()) {
                    let colorless_text: String = remove_color(said_text).trim().to_string();

                    let send_entity = SendEntity::from(&entity);

                    let self_pos = self_entity.get_position();
                    let self_rot_yaw = self_entity.get_rot()[1];

                    let chatsounds = self.chatsounds.clone();
                    let entity_emitters = self.entity_emitters.clone();

                    // it doesn't matter if these are out of order so we just spawn
                    FuturesModule::spawn_future(async move {
                        play_chatsound(
                            colorless_text,
                            real_name,
                            send_entity,
                            self_pos,
                            self_rot_yaw,
                            chatsounds,
                            entity_emitters,
                        )
                        .await;
                    });
                } else {
                    print("couldn't entities.get(ENTITY_SELF_ID)");
                }
            }
        }
    }
}

pub async fn play_chatsound(
    sentence: String,
    real_name: String,
    entity: SendEntity,
    self_pos: Vec3,
    self_rot_yaw: f32,
    chatsounds: FutureShared<Option<Chatsounds>>,
    entity_emitters: ThreadShared<Vec<EntityEmitter>>,
) {
    let mut chatsounds = chatsounds.lock().await;
    let chatsounds = chatsounds.as_mut().unwrap();

    if chatsounds.volume() == 0.0 {
        // don't even play the sound if we have 0 volume
        return;
    }

    if sentence.to_lowercase() == "sh" {
        chatsounds.stop_all();
        entity_emitters.lock().unwrap().clear();
        return;
    }

    if entity.id == ENTITY_SELF_ID {
        // if self entity, play 2d sound
        let _ignore_error = chatsounds.play(&sentence, get_rng(&real_name)).await;
    } else {
        let (emitter_pos, left_ear_pos, right_ear_pos) =
            EntityEmitter::coords_to_sink_positions(entity.pos, self_pos, self_rot_yaw);

        if let Ok(sink) = chatsounds
            .play_spatial(
                &sentence,
                get_rng(&real_name),
                emitter_pos,
                left_ear_pos,
                right_ear_pos,
            )
            .await
        {
            // don't print other's errors
            entity_emitters
                .lock()
                .unwrap()
                .push(EntityEmitter::new(entity.id, &sink));
        }
    }
}

impl IncomingEventListener for ChatsoundsEventListener {
    fn handle_incoming_event(&mut self, event: &IncomingEvent) {
        match event.clone() {
            IncomingEvent::ChatReceived(message, msg_type) => {
                self.handle_chat_received(message, msg_type)
            }

            IncomingEvent::Tick => {
                // update positions on emitters

                let mut entity_emitters = self.entity_emitters.lock().unwrap();

                let mut to_remove = Vec::with_capacity(entity_emitters.len());
                for (i, emitter) in entity_emitters.iter_mut().enumerate() {
                    if !emitter.update(&mut self.entities) {
                        to_remove.push(i);
                    }
                }

                // TODO can't you just use a for remove_id in ().rev()
                if !to_remove.is_empty() {
                    for i in (0..entity_emitters.len()).rev() {
                        if to_remove.contains(&i) {
                            entity_emitters.remove(i);
                        }
                    }
                }
            }

            _ => {}
        }
    }
}
