use std::{collections::HashMap, ffi::CString, os::raw::c_char};

use classicube_sys::{
    keybindNames, InputButtons, Input_Names, KeyBind_Defaults, Options_Get, Options_Set,
    OwnedString, STRING_SIZE,
};

use crate::modules::Module;

pub struct OptionModule {
    pub open_chat_key: Option<InputButtons>,
    pub send_chat_key: Option<InputButtons>,
}

impl OptionModule {
    pub fn new() -> Self {
        Self {
            open_chat_key: None,
            send_chat_key: None,
        }
    }

    pub fn get_key_from_input_name<S: AsRef<str>>(s: S) -> Option<InputButtons> {
        let s = s.as_ref();

        Input_Names
            .iter()
            .position(|&item| item == s)
            .map(|n| n as InputButtons)
    }

    pub fn get<S: Into<Vec<u8>>>(&self, key: S) -> Option<String> {
        let c_key = CString::new(key).unwrap();
        let c_default = CString::new("").unwrap();

        let mut buffer: [c_char; (STRING_SIZE as usize) + 1] = [0; (STRING_SIZE as usize) + 1];
        let mut cc_string_value = classicube_sys::cc_string {
            buffer: buffer.as_mut_ptr(),
            capacity: STRING_SIZE as u16,
            length: 0,
        };

        unsafe {
            Options_Get(c_key.as_ptr(), &mut cc_string_value, c_default.as_ptr());
        }

        let string_value = cc_string_value.to_string();

        if string_value.is_empty() {
            None
        } else {
            Some(string_value)
        }
    }

    pub fn set<S: Into<Vec<u8>>>(&mut self, key: S, value: String) {
        let c_key = CString::new(key).unwrap();

        let cc_string_value = OwnedString::new(value);

        unsafe {
            Options_Set(c_key.as_ptr(), cc_string_value.as_cc_string());
        }
    }

    fn get_all_keybinds(&self) -> HashMap<&'static str, InputButtons> {
        let mut map = HashMap::with_capacity(keybindNames.len());

        for (i, keybind_name) in keybindNames.iter().copied().enumerate() {
            let option_name = format!("key-{}", keybind_name);

            let key = self
                .get(option_name)
                .and_then(OptionModule::get_key_from_input_name)
                .unwrap_or_else(|| KeyBind_Defaults[i] as InputButtons);

            map.insert(keybind_name, key);
        }

        map
    }
}

impl Module for OptionModule {
    fn load(&mut self) {
        let keybinds = self.get_all_keybinds();

        self.open_chat_key = keybinds.get("Chat").cloned();
        self.send_chat_key = keybinds.get("SendChat").cloned();
    }

    fn unload(&mut self) {}
}
