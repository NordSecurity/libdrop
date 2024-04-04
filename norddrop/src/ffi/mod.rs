pub mod types;
pub mod uni;

use std::{collections::HashMap, ffi::CString, fmt};

use slog::KV;

use self::types::{norddrop_event_cb, norddrop_logger_cb, norddrop_pubkey_cb};

/// Check if res is ok, else return early by converting Error into
/// norddrop_result

struct KeyValueSerializer<'a> {
    rec: &'a slog::Record<'a>,
    kv: HashMap<slog::Key, String>,
}

impl<'a> slog::Serializer for KeyValueSerializer<'a> {
    fn emit_arguments(&mut self, key: slog::Key, val: &fmt::Arguments) -> slog::Result {
        self.kv.insert(key, val.to_string());
        Ok(())
    }
}

impl<'a> KeyValueSerializer<'a> {
    fn new(rec: &'a slog::Record) -> Self {
        KeyValueSerializer {
            rec,
            kv: HashMap::new(),
        }
    }

    fn msg(self) -> String {
        let file = self.rec.file();
        let line = self.rec.line();
        let msg = self.rec.msg();
        let kv = self.kv;

        if kv.is_empty() {
            return format!("{file}:{line} {msg}");
        }

        let kv_str = kv.iter().fold(String::new(), |mut acc, (k, v)| {
            acc.push_str(&format!("{} => {}, ", k, v));
            acc
        });

        format!("{file}:{line} {msg} @ [{kv_str}]")
    }
}

impl slog::Drain for norddrop_logger_cb {
    type Ok = ();
    type Err = ();

    /// Log a record.
    /// record.kv() contains key:value pairs inside of macro calls for logging
    /// with a `a => b` syntax
    /// the key:val pair inside of arguments is of the parent logger
    fn log(&self, record: &slog::Record, _: &slog::OwnedKVList) -> Result<Self::Ok, Self::Err> {
        if !self.is_enabled(record.level()) {
            return Ok(());
        }

        let kv = record.kv();

        let mut serializer = KeyValueSerializer::new(record);

        let _ = kv.serialize(record, &mut serializer);

        if let Ok(cstr) = CString::new(serializer.msg()) {
            unsafe { (self.cb)(self.ctx, record.level().into(), cstr.as_ptr()) };
        }

        Ok(())
    }
}
