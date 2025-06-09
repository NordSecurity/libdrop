pub struct MockImpl;

impl super::Moose for MockImpl {
    fn event_init(&self, _: crate::InitEventData) {}
    fn event_transfer_intent(&self, _: crate::TransferIntentEventData) {}
    fn event_transfer_intent_received(&self, _: crate::TransferIntentReceivedEventData) {}
    fn event_transfer_state(&self, _: crate::TransferStateEventData) {}
    fn event_transfer_file(&self, _: crate::TransferFileEventData) {}
    fn developer_exception(&self, _: crate::DeveloperExceptionEventData) {}
}
