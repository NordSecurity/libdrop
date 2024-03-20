use std::{
    sync::mpsc::{sync_channel, SyncSender},
    time::Duration,
};

use anyhow::Context;
use mooselibdropapp as moose;
use serde_json::Value;
use slog::{error, info, warn, Logger};

use crate::{TransferDirection, TransferFilePhase, MOOSE_VALUE_NONE};

const DROP_MOOSE_APP_NAME: &str = "norddrop";

/// Version of the tracker used, should be updated every time the tracker
/// library is updated
const DROP_MOOSE_TRACKER_VERSION: &str = "4.0.1";

pub struct MooseImpl {
    logger: slog::Logger,
    task_abort_channel: SyncSender<()>,
    context_update_task: Option<std::thread::JoinHandle<()>>,
}

struct MooseInitCallback {
    logger: slog::Logger,
    init_tx: SyncSender<Result<moose::TrackerState, moose::MooseError>>,
}

impl moose::InitCallback for MooseInitCallback {
    fn after_init(&self, result_code: &Result<moose::TrackerState, moose::MooseError>) {
        info!(self.logger, "[Moose] Init callback: {:?}", result_code);
        let _ = self.init_tx.send(*result_code);
    }
}

struct MooseErrorCallback {
    logger: slog::Logger,
}

impl moose::ErrorCallback for MooseErrorCallback {
    fn on_error(
        &self,
        _error_level: moose::MooseErrorLevel,
        error_code: moose::MooseError,
        msg: &str,
    ) {
        error!(
            self.logger,
            "[Moose] Error callback {:?}: {:?}", error_code, msg
        );
    }
}

macro_rules! moose_debug {
    (
        $logger:expr,
        $result:ident,
        $func:expr
    ) => {
        if let Some(error) = $result.as_ref().err() {
            warn!($logger, "[Moose] Error: {:?} on call to `{}`", error, $func);
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
        lib_version: String,
        app_version: String,
        prod: bool,
    ) -> anyhow::Result<Self> {
        let (tx, rx) = sync_channel(1);

        let res = moose::init(
            event_path,
            DROP_MOOSE_APP_NAME.to_string(),
            lib_version,
            DROP_MOOSE_TRACKER_VERSION.to_string(),
            prod,
            Box::new(MooseInitCallback {
                logger: logger.clone(),
                init_tx: tx,
            }),
            Box::new(MooseErrorCallback {
                logger: logger.clone(),
            }),
        );

        moose_debug!(logger, res, "init");
        res.context("Failed to initialize moose")?;

        let res = rx
            .recv_timeout(Duration::from_secs(2))
            .context("Failed to receive moose init callback result, channel timed out")?;
        moose_debug!(logger, res, "init");

        anyhow::ensure!(res.is_ok(), "Failed to initialize moose: {:?}", res.err());
        populate_context(&logger, app_version.clone());

        let (tx, rx) = sync_channel(1);
        let task_logger = logger.clone();
        let context_update_task = std::thread::spawn(move || loop {
            match rx.recv_timeout(Duration::from_secs(60 * 10)) {
                Ok(_) | Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    populate_context(&task_logger, app_version.clone())
                }
            }
        });

        Ok(Self {
            logger,
            task_abort_channel: tx,
            context_update_task: Some(context_update_task),
        })
    }
}

impl Drop for MooseImpl {
    fn drop(&mut self) {
        moose!(self.logger, moose_deinit);

        if let Some(context_update_task) = self.context_update_task.take() {
            let _ = self.task_abort_channel.send(());
            let _ = context_update_task.join();
        }
    }
}

impl super::Moose for MooseImpl {
    fn event_init(&self, data: crate::InitEventData) {
        moose!(
            self.logger,
            send_serviceQuality_initialization_init,
            data.result,
            data.init_duration
        );
    }

    fn event_transfer_intent(&self, data: crate::TransferIntentEventData) {
        moose!(
            self.logger,
            send_serviceQuality_transfer_intent,
            data.extensions,
            data.mime_types,
            data.file_count,
            data.path_ids,
            data.file_sizes,
            data.transfer_id,
            data.transfer_size
        );
    }

    fn event_transfer_state(&self, data: crate::TransferStateEventData) {
        moose!(
            self.logger,
            send_serviceQuality_transfer_state,
            data.result,
            data.protocol_version,
            data.transfer_id
        );
    }

    fn event_transfer_file(&self, data: crate::TransferFileEventData) {
        moose!(
            self.logger,
            send_serviceQuality_transfer_file,
            data.result,
            data.phase.into(),
            data.path_id.into(),
            data.direction.into(),
            data.transfer_id,
            data.transfer_time,
            data.transferred
        );
    }

    fn developer_exception(&self, data: crate::DeveloperExceptionEventData) {
        moose!(
            self.logger,
            send_developer_exceptionHandling_catchException,
            MOOSE_VALUE_NONE,
            data.code,
            data.note,
            data.message,
            data.name
        );
    }

    fn developer_exception_with_value(&self, data: crate::DeveloperExceptionWithValueEventData) {
        moose!(
            self.logger,
            send_developer_exceptionHandling_catchException,
            data.arbitrary_value,
            data.code,
            data.note,
            data.message,
            data.name
        );
    }
}

impl From<TransferDirection> for moose::LibdropappTransferDirection {
    fn from(direction: TransferDirection) -> Self {
        match direction {
            TransferDirection::Upload => {
                moose::LibdropappTransferDirection::LibdropappTransferDirectionUpload
            }
            TransferDirection::Download => {
                moose::LibdropappTransferDirection::LibdropappTransferDirectionDownload
            }
        }
    }
}

impl From<TransferFilePhase> for moose::LibdropappFilePhase {
    fn from(phase: TransferFilePhase) -> Self {
        match phase {
            TransferFilePhase::Finished => moose::LibdropappFilePhase::LibdropappFilePhaseFinished,
            TransferFilePhase::Paused => moose::LibdropappFilePhase::LibdropappFilePhasePaused,
        }
    }
}

fn populate_context(logger: &Logger, app_version: String) {
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
        let (device, user) = parse_foreign_context(&foreign_context);

        set_context_fields!(
            set_context_device_type, device.x_type;
            set_context_device_model, device.model;
            set_context_device_brand, device.brand;
            set_context_device_fp, device.fp;
            set_context_device_resolution, device.resolution;
            set_context_device_os, device.os;
            set_context_device_location_city, device.location.city;
            set_context_device_location_country, device.location.country;
            set_context_device_location_region, device.location.region;
            set_context_device_timeZone, device.time_zone;
            set_context_device_ram_module, device.ram.module;
            set_context_device_ram_totalMemory, device.ram.total_memory;
            set_context_device_ram_availableMemory, device.ram.total_memory;
            set_context_device_storage_mediaType, device.storage.media_type
        );

        set_context_fields!(
            set_context_user_fp, user.fp;
            set_context_user_subscription_currentState_activationDate, user.subscription.current_state.activation_date;
            set_context_user_subscription_currentState_frequencyInterval, user.subscription.current_state.frequency_interval;
            set_context_user_subscription_currentState_frequencyUnit, user.subscription.current_state.frequency_unit;
            set_context_user_subscription_currentState_isActive, user.subscription.current_state.is_active;
            set_context_user_subscription_currentState_isNewCustomer, user.subscription.current_state.is_new_customer;
            set_context_user_subscription_currentState_merchantId, user.subscription.current_state.merchant_id;
            set_context_user_subscription_currentState_paymentAmount, user.subscription.current_state.payment_amount;
            set_context_user_subscription_currentState_paymentCurrency, user.subscription.current_state.payment_currency;
            set_context_user_subscription_currentState_paymentProvider, user.subscription.current_state.payment_provider;
            set_context_user_subscription_currentState_paymentStatus, user.subscription.current_state.payment_status;
            set_context_user_subscription_currentState_planId, user.subscription.current_state.plan_id;
            set_context_user_subscription_currentState_planType, user.subscription.current_state.plan_type;
            set_context_user_subscription_currentState_subscriptionStatus, user.subscription.current_state.subscription_status;
            set_context_user_subscription_history, user.subscription.history
        );

        set_context_fields!(
            set_context_application_config_currentState_nordvpnappVersion,
            Some(app_version)
        );
    } else {
        warn!(
            logger,
            "[Moose] Could not fetch {} tracker device context", foreign_tracker_name
        );
    }
}

fn parse_foreign_context(
    foreign_context: &str,
) -> (moose::LibdropappContextDevice, moose::LibdropappContextUser) {
    let mut device = moose::LibdropappContextDevice {
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

    let mut user = moose::LibdropappContextUser {
        fp: None,
        subscription: moose::LibdropappContextUserSubscription {
            current_state: moose::LibdropappContextUserSubscriptionCurrentState {
                activation_date: None,
                frequency_interval: None,
                frequency_unit: None,
                is_active: None,
                is_new_customer: None,
                merchant_id: None,
                payment_amount: None,
                payment_currency: None,
                payment_provider: None,
                payment_status: None,
                plan_id: None,
                plan_type: None,
                subscription_status: None,
            },
            history: None,
        },
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

    let get_json_f32 = |json_value: &Value, field_name: &str| {
        if let Some(Value::Number(val)) = json_value.get(field_name) {
            val.as_f64().map(|num| num as f32)
        } else {
            None
        }
    };

    let get_json_bool = |json_value: &Value, field_name: &str| {
        if let Some(Value::Bool(val)) = json_value.get(field_name) {
            Some(val.clone())
        } else {
            None
        }
    };

    if let Ok(value) = serde_json::from_str::<Value>(foreign_context) {
        if let Some(foreign_device) = value.get("device") {
            device.brand = get_json_string(foreign_device, "brand");
            device.model = get_json_string(foreign_device, "model");
            device.fp = get_json_string(foreign_device, "fp");
            device.resolution = get_json_string(foreign_device, "resolution");
            device.os = get_json_string(foreign_device, "os");
            device.time_zone = get_json_string(foreign_device, "time_zone");

            if let Some(x_type) = foreign_device.get("type") {
                if let Ok(x_type) =
                    serde_json::from_value::<moose::LibdropappDeviceType>(x_type.clone())
                {
                    device.x_type = Some(x_type);
                }
            }

            if let Some(location) = foreign_device.get("location") {
                device.location.city = get_json_string(location, "city");
                device.location.country = get_json_string(location, "country");
                device.location.region = get_json_string(location, "region");
            }

            if let Some(ram) = foreign_device.get("ram") {
                device.ram.module = get_json_string(ram, "module");
                device.ram.total_memory = get_json_i32(ram, "total_memory");
                device.ram.available_memory = get_json_i32(ram, "available_memory");
            }

            if let Some(storage) = foreign_device.get("storage") {
                device.storage.media_type = get_json_string(storage, "media_type");
            }
        }

        if let Some(foreign_user) = value.get("user") {
            user.fp = get_json_string(foreign_user, "fp");

            if let Some(subscription) = foreign_user.get("subscription") {
                if let Some(current_state) = subscription.get("current_state") {
                    user.subscription.current_state.activation_date =
                        get_json_string(current_state, "activation_date");
                    user.subscription.current_state.frequency_interval =
                        get_json_i32(current_state, "frequency_interval");
                    user.subscription.current_state.frequency_unit =
                        get_json_string(current_state, "frequency_unit");
                    user.subscription.current_state.is_active =
                        get_json_bool(current_state, "is_active");
                    user.subscription.current_state.is_new_customer =
                        get_json_bool(current_state, "is_new_customer");
                    user.subscription.current_state.merchant_id =
                        get_json_i32(current_state, "merchant_id");
                    user.subscription.current_state.payment_amount =
                        get_json_f32(current_state, "payment_amount");
                    user.subscription.current_state.payment_currency =
                        get_json_string(current_state, "payment_currency");
                    user.subscription.current_state.payment_provider =
                        get_json_string(current_state, "payment_provider");
                    user.subscription.current_state.payment_status =
                        get_json_string(current_state, "payment_status");
                    user.subscription.current_state.plan_id =
                        get_json_i32(current_state, "plan_id");
                    user.subscription.current_state.plan_type =
                        get_json_string(current_state, "plan_type");
                    user.subscription.current_state.subscription_status =
                        get_json_string(current_state, "subscription_status");
                }
                user.subscription.history = get_json_string(subscription, "history");
            }
        }
    }

    (device, user)
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
            },
            "user": {
                "fp": "finger",
                "subscription": {
                    "current_state": {
                        "activation_date": "now",
                        "frequency_interval": 1,
                        "frequency_unit": "seconds",
                        "is_active": false,
                        "is_new_customer": true,
                        "merchant_id": 4,
                        "payment_amount": 32.34,
                        "payment_currency": "gold",
                        "payment_provider": "someone",
                        "payment_status": "rejected",
                        "plan_id": 0,
                        "plan_type": "good",
                        "subscription_status": "is good"
                    },
                    "history": "gone"
                }
            }
        }"#;

        let (device, user) = parse_foreign_context(json);

        assert_eq!(device.brand, Some(String::from("brend")));
        assert_eq!(device.model, Some(String::from("best")));
        assert_eq!(device.fp, Some(String::from("pingerfrint")));
        assert_eq!(device.resolution, Some(String::from("256x144@2hz")));
        assert_eq!(device.os, Some(String::from("TempleOS")));
        assert_eq!(device.location.city, Some(String::from("nordo")));
        assert_eq!(device.location.country, Some(String::from("troba")));
        assert_eq!(device.location.region, Some(String::from("redacted")));
        assert_eq!(device.time_zone, Some(String::from("current")));
        assert_eq!(device.ram.module, Some(String::from("downloaded")));
        assert_eq!(device.ram.total_memory, Some(6i32));
        assert_eq!(device.ram.available_memory, Some(9i32));
        assert_eq!(device.storage.media_type, Some(String::from("vidyja")));
        assert!(device
            .x_type
            .map(|t| t == moose::LibdropappDeviceType::LibdropappDeviceTypeMobile)
            .unwrap_or(false));

        assert_eq!(user.fp, Some(String::from("finger")));
        assert_eq!(
            user.subscription.current_state.activation_date,
            Some(String::from("now"))
        );
        assert_eq!(user.subscription.current_state.frequency_interval, Some(1));
        assert_eq!(
            user.subscription.current_state.frequency_unit,
            Some(String::from("seconds"))
        );
        assert_eq!(user.subscription.current_state.is_active, Some(false));
        assert_eq!(user.subscription.current_state.is_new_customer, Some(true));
        assert_eq!(user.subscription.current_state.merchant_id, Some(4));
        assert_eq!(user.subscription.current_state.payment_amount, Some(32.34));
        assert_eq!(
            user.subscription.current_state.payment_currency,
            Some(String::from("gold"))
        );
        assert_eq!(
            user.subscription.current_state.payment_provider,
            Some(String::from("someone"))
        );
        assert_eq!(
            user.subscription.current_state.payment_status,
            Some(String::from("rejected"))
        );
        assert_eq!(user.subscription.current_state.plan_id, Some(0));
        assert_eq!(
            user.subscription.current_state.plan_type,
            Some(String::from("good"))
        );
        assert_eq!(
            user.subscription.current_state.subscription_status,
            Some(String::from("is good"))
        );
        assert_eq!(user.subscription.history, Some(String::from("gone")));
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
            },
            "user": {
                "fp": "finger",
                "subscription": {
                    "current_state": {
                        "is_active": false
                    }
                }
            }
        }"#;

        let (device, user) = parse_foreign_context(json);

        assert_eq!(device.brand, Some(String::from("brend")));
        assert_eq!(device.model, Some(String::from("best")));
        assert_eq!(device.fp, Some(String::from("pingerfrint")));
        assert_eq!(device.resolution, None);
        assert_eq!(device.os, Some(String::from("TempleOS")));
        assert_eq!(device.location.city, None);
        assert_eq!(device.location.country, None);
        assert_eq!(device.location.region, None);
        assert_eq!(device.time_zone, Some(String::from("current")));
        assert_eq!(device.ram.module, Some(String::from("downloaded")));
        assert_eq!(device.ram.total_memory, Some(6i32));
        assert_eq!(device.ram.available_memory, Some(9i32));
        assert_eq!(device.storage.media_type, Some(String::from("vidyja")));
        assert!(device.x_type.is_none());

        assert_eq!(user.fp, Some(String::from("finger")));
        assert_eq!(user.subscription.current_state.activation_date, None);
        assert_eq!(user.subscription.current_state.frequency_interval, None);
        assert_eq!(user.subscription.current_state.frequency_unit, None);
        assert_eq!(user.subscription.current_state.is_active, Some(false));
        assert_eq!(user.subscription.current_state.is_new_customer, None);
        assert_eq!(user.subscription.current_state.merchant_id, None);
        assert_eq!(user.subscription.current_state.payment_amount, None);
        assert_eq!(user.subscription.current_state.payment_currency, None);
        assert_eq!(user.subscription.current_state.payment_provider, None);
        assert_eq!(user.subscription.current_state.payment_status, None);
        assert_eq!(user.subscription.current_state.plan_id, None);
        assert_eq!(user.subscription.current_state.plan_type, None);
        assert_eq!(user.subscription.current_state.subscription_status, None);
        assert_eq!(user.subscription.history, None);
    }

    #[test]
    fn test_moose_context_deserialize_garbage() {
        let json = "i'll have two number 9s, a number 9 large, a number 6 with extra dip, a \
                    number 7, two number 45s, one with cheese, and a large soda";

        let (device, user) = parse_foreign_context(json);

        assert_eq!(device.brand, None);
        assert_eq!(device.model, None);
        assert_eq!(device.fp, None);
        assert_eq!(device.resolution, None);
        assert_eq!(device.os, None);
        assert_eq!(device.location.city, None);
        assert_eq!(device.location.country, None);
        assert_eq!(device.location.region, None);
        assert_eq!(device.time_zone, None);
        assert_eq!(device.ram.module, None);
        assert_eq!(device.ram.total_memory, None);
        assert_eq!(device.ram.available_memory, None);
        assert_eq!(device.storage.media_type, None);
        assert!(device.x_type.is_none());

        assert_eq!(user.fp, None);
        assert_eq!(user.subscription.current_state.activation_date, None);
        assert_eq!(user.subscription.current_state.frequency_interval, None);
        assert_eq!(user.subscription.current_state.frequency_unit, None);
        assert_eq!(user.subscription.current_state.is_active, None);
        assert_eq!(user.subscription.current_state.is_new_customer, None);
        assert_eq!(user.subscription.current_state.merchant_id, None);
        assert_eq!(user.subscription.current_state.payment_amount, None);
        assert_eq!(user.subscription.current_state.payment_currency, None);
        assert_eq!(user.subscription.current_state.payment_provider, None);
        assert_eq!(user.subscription.current_state.payment_status, None);
        assert_eq!(user.subscription.current_state.plan_id, None);
        assert_eq!(user.subscription.current_state.plan_type, None);
        assert_eq!(user.subscription.current_state.subscription_status, None);
        assert_eq!(user.subscription.history, None);
    }
}
