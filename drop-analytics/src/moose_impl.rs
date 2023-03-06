use anyhow::Context;
use mooselibdropapp as moose;
use serde_json::Value;
use slog::{warn, Logger};

const DROP_MOOSE_APP_NAME: &str = "norddrop";

/// Version of the tracker used, should be updated everytime the tracker library
/// is updated
const DROP_MOOSE_TRACKER_VERSION: &str = "0.0.2";

const MOOSE_STATUS_SUCCESS: i32 = 0;

pub struct MooseImpl {
    logger: slog::Logger,
}

macro_rules! moose_debug {
    (
        $logger:expr,
        $result:ident,
        $func:expr
    ) => {
        if let Some(error) = $result.as_ref().err() {
            warn!($logger, "[Moose] Error: {} on call to `{}`", error, $func);
        }
    };
}

macro_rules! moose {
    (
        $logger:expr,
        $func:ident
        $(,$arg:expr)*
    ) => {
        {
            let result = moose::$func($($arg),*);
            moose_debug!($logger, result, stringify!($func));
        }
    };
}

impl MooseImpl {
    pub fn new(
        logger: Logger,
        event_path: String,
        app_version: String,
        prod: bool,
    ) -> anyhow::Result<Self> {
        let res = moose::init(
            event_path,
            DROP_MOOSE_APP_NAME.to_string(),
            app_version,
            DROP_MOOSE_TRACKER_VERSION.to_string(),
            prod,
        );

        moose_debug!(logger, res, "init");
        res.context("Failed to initialize moose")?;

        populate_context(&logger);

        Ok(Self { logger })
    }
}

impl Drop for MooseImpl {
    fn drop(&mut self) {
        moose!(self.logger, moose_deinit);
    }
}

impl super::Moose for MooseImpl {
    fn service_quality_initialization_init(&self, res: Result<(), i32>, phase: super::Phase) {
        let errno = match res {
            Ok(()) => MOOSE_STATUS_SUCCESS,
            Err(err) => err,
        };

        moose!(
            self.logger,
            send_serviceQuality_initialization_init,
            errno,
            phase.into()
        );
    }

    fn service_quality_transfer_batch(
        &self,
        phase: super::Phase,
        files_count: i32,
        size_of_files_list: String,
        transfer_id: String,
        transfer_size: i32,
    ) {
        moose!(
            self.logger,
            send_serviceQuality_transfer_batch,
            phase.into(),
            files_count,
            size_of_files_list,
            transfer_id,
            transfer_size
        );
    }

    fn service_quality_transfer_file(
        &self,
        res: Result<(), i32>,
        phase: crate::Phase,
        transfer_id: String,
        transfer_size: Option<i32>,
        transfer_time: i32,
    ) {
        let errno = match res {
            Ok(()) => MOOSE_STATUS_SUCCESS,
            Err(err) => err,
        };

        moose!(
            self.logger,
            send_serviceQuality_transfer_file,
            errno,
            phase.into(),
            transfer_id,
            transfer_size.unwrap_or(0),
            transfer_time
        );
    }
}

impl From<super::Phase> for mooselibdropapp::LibdropappEventPhase {
    fn from(value: super::Phase) -> Self {
        match value {
            crate::Phase::Start => mooselibdropapp::LibdropappEventPhase::LibdropappEventPhaseStart,
            crate::Phase::End => mooselibdropapp::LibdropappEventPhase::LibdropappEventPhaseEnd,
        }
    }
}

fn populate_context(logger: &Logger) {
    macro_rules! set_context_fields {
        ( $( $func:ident, $field:expr );* ) => {
            $(
                if let Some(val) = $field {
                    moose!(logger, $func, val);
                } else {
                    warn!(
                        logger,
                        "[Moose] {} device context field is empty",
                        stringify!($field)
                    )
                }
            )*
        };
    }

    let foreign_tracker_name = "nordvpnapp";

    if let Ok(foreign_context) = moose::fetch_specific_context(foreign_tracker_name) {
        let context = parse_foreign_context(&foreign_context);
        set_context_fields!(
            set_context_device_brand, context.brand;
            set_context_device_type, context.x_type;
            set_context_device_model, context.model;
            set_context_device_fp, context.fp;
            set_context_device_resolution, context.resolution;
            set_context_device_os, context.os;
            set_context_device_location_city, context.location.city;
            set_context_device_location_country, context.location.country;
            set_context_device_location_region, context.location.region;
            set_context_device_timeZone, context.time_zone;
            set_context_device_ram_module, context.ram.module;
            set_context_device_ram_totalMemory, context.ram.total_memory;
            set_context_device_ram_availableMemory, context.ram.total_memory;
            set_context_device_storage_mediaType, context.storage.media_type
        );
    } else {
        warn!(
            logger,
            "[Moose] Could not fetch {} tracker device context", foreign_tracker_name
        );
    }
}

fn parse_foreign_context(foreign_context: &str) -> moose::LibdropappContextDevice {
    let mut context = moose::LibdropappContextDevice {
        brand: None,
        x_type: None,
        model: None,
        fp: None,
        resolution: None,
        os: None,
        location: moose::LibdropappContextDeviceLocation {
            country: None,
            city: None,
            region: None,
        },
        time_zone: None,
        ram: moose::LibdropappContextDeviceRam {
            module: None,
            total_memory: None,
            available_memory: None,
        },
        storage: moose::LibdropappContextDeviceStorage { media_type: None },
    };

    let get_json_string = |json_value: &Value, field_name: &str| {
        if let Some(Value::String(val)) = json_value.get(field_name) {
            Some(val.clone())
        } else {
            None
        }
    };

    let get_json_i32 = |json_value: &Value, field_name: &str| {
        if let Some(Value::Number(val)) = json_value.get(field_name) {
            val.as_i64().map(|num| num as i32)
        } else {
            None
        }
    };

    if let Ok(value) = serde_json::from_str::<Value>(foreign_context) {
        if let Some(device) = value.get("device") {
            context.brand = get_json_string(device, "brand");
            context.model = get_json_string(device, "model");
            context.fp = get_json_string(device, "fp");
            context.resolution = get_json_string(device, "resolution");
            context.os = get_json_string(device, "os");
            context.time_zone = get_json_string(device, "time_zone");

            if let Some(x_type) = device.get("type") {
                if let Ok(x_type) =
                    serde_json::from_value::<moose::LibdropappDeviceType>(x_type.clone())
                {
                    context.x_type = Some(x_type);
                }
            }

            if let Some(location) = device.get("location") {
                context.location.city = get_json_string(location, "city");
                context.location.country = get_json_string(location, "country");
                context.location.region = get_json_string(location, "region");
            }

            if let Some(ram) = device.get("ram") {
                context.ram.module = get_json_string(ram, "module");
                context.ram.total_memory = get_json_i32(ram, "total_memory");
                context.ram.available_memory = get_json_i32(ram, "available_memory");
            }

            if let Some(storage) = device.get("storage") {
                context.storage.media_type = get_json_string(storage, "media_type");
            }
        }
    }

    context
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_moose_context_deserialize_complete() {
        let json = r#"{
            "device": {
                "brand": "brend",
                "type": "mobile",
                "model": "best",
                "fp": "pingerfrint",
                "resolution": "256x144@2hz",
                "os": "TempleOS",
                "location": {
                    "city": "nordo",
                    "country": "troba",
                    "region": "redacted"
                },
                "time_zone": "current",
                "ram": {
                    "module": "downloaded",
                    "total_memory": 6,
                    "available_memory": 9
                },
                "storage": {
                    "media_type": "vidyja"
                }
            }
        }"#;

        let parsed = parse_foreign_context(json);

        assert_eq!(parsed.brand, Some(String::from("brend")));
        assert_eq!(parsed.model, Some(String::from("best")));
        assert_eq!(parsed.fp, Some(String::from("pingerfrint")));
        assert_eq!(parsed.resolution, Some(String::from("256x144@2hz")));
        assert_eq!(parsed.os, Some(String::from("TempleOS")));
        assert_eq!(parsed.location.city, Some(String::from("nordo")));
        assert_eq!(parsed.location.country, Some(String::from("troba")));
        assert_eq!(parsed.location.region, Some(String::from("redacted")));
        assert_eq!(parsed.time_zone, Some(String::from("current")));
        assert_eq!(parsed.ram.module, Some(String::from("downloaded")));
        assert_eq!(parsed.ram.total_memory, Some(6i32));
        assert_eq!(parsed.ram.available_memory, Some(9i32));
        assert_eq!(parsed.storage.media_type, Some(String::from("vidyja")));
        assert!(parsed
            .x_type
            .map(|t| t == moose::LibdropappDeviceType::LibdropappDeviceTypeMobile)
            .unwrap_or(false));
    }

    #[test]
    fn test_moose_context_deserialze_partial() {
        let json = r#"{
            "device": {
                "brand": "brend",
                "model": "best",
                "fp": "pingerfrint",
                "os": "TempleOS",
                "time_zone": "current",
                "ram": {
                    "module": "downloaded",
                    "total_memory": 6,
                    "available_memory": 9
                },
                "storage": {
                    "media_type": "vidyja"
                }
            }
        }"#;

        let parsed = parse_foreign_context(json);

        assert_eq!(parsed.brand, Some(String::from("brend")));
        assert_eq!(parsed.model, Some(String::from("best")));
        assert_eq!(parsed.fp, Some(String::from("pingerfrint")));
        assert_eq!(parsed.resolution, None);
        assert_eq!(parsed.os, Some(String::from("TempleOS")));
        assert_eq!(parsed.location.city, None);
        assert_eq!(parsed.location.country, None);
        assert_eq!(parsed.location.region, None);
        assert_eq!(parsed.time_zone, Some(String::from("current")));
        assert_eq!(parsed.ram.module, Some(String::from("downloaded")));
        assert_eq!(parsed.ram.total_memory, Some(6i32));
        assert_eq!(parsed.ram.available_memory, Some(9i32));
        assert_eq!(parsed.storage.media_type, Some(String::from("vidyja")));
        assert!(parsed.x_type.is_none());
    }

    #[test]
    fn test_moose_context_deserialize_garbage() {
        let json = "i'll have two number 9s, a number 9 large, a number 6 with extra dip, a \
                    number 7, two number 45s, one with cheese, and a large soda";

        let parsed = parse_foreign_context(json);

        assert_eq!(parsed.brand, None);
        assert_eq!(parsed.model, None);
        assert_eq!(parsed.fp, None);
        assert_eq!(parsed.resolution, None);
        assert_eq!(parsed.os, None);
        assert_eq!(parsed.location.city, None);
        assert_eq!(parsed.location.country, None);
        assert_eq!(parsed.location.region, None);
        assert_eq!(parsed.time_zone, None);
        assert_eq!(parsed.ram.module, None);
        assert_eq!(parsed.ram.total_memory, None);
        assert_eq!(parsed.ram.available_memory, None);
        assert_eq!(parsed.storage.media_type, None);
        assert!(parsed.x_type.is_none());
    }
}
