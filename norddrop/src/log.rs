use std::{
    collections::HashMap,
    fmt,
    panic::{RefUnwindSafe, UnwindSafe},
};

use slog::{o, Drain, KV};

pub fn create(callback: Box<dyn crate::Logger>) -> slog::Logger {
    let level = callback.level();
    slog::Logger::root(
        super::log::Log(callback).filter_level(level.into()).fuse(),
        o!(),
    )
}

struct Log(pub Box<dyn crate::Logger>);

impl UnwindSafe for Log {}
impl RefUnwindSafe for Log {}

struct KeyValueSerializer<'a> {
    rec: &'a slog::Record<'a>,
    kv: HashMap<slog::Key, String>,
}

impl slog::Serializer for KeyValueSerializer<'_> {
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

impl Drain for Log {
    type Ok = ();
    type Err = slog::Never;

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

        self.0.on_log(record.level().into(), serializer.msg());
        Ok(())
    }
}
