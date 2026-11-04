//! Detached observations of work issued through Nara's native preparation/program paths.

/// Work reached during the current original Host transaction, including rejected attempts.
///
/// Queue fields count calls that returned from Nara's preparation helper, with the complete
/// supplied byte-slice length. Later native validation, encoding or submission failure does not
/// refund that work. A panicking Queue call is not counted. These are issued requests, not
/// hardware transfer bytes or completion evidence. Empty returned writes still count as calls.
///
/// Initialized buffers count successful nonempty initial mapped copies before later custody
/// admission. Payload bytes exclude padding; storage bytes include four-byte alignment.
/// Copy fields count whole-resource Preserve commands encoded by Nara's program initializer,
/// even when the encoder is later discarded. Texture bytes use the admitted native format.
///
/// Counts saturate and reset at the next Host transaction. They exclude direct native callback
/// work, third-party renderer work, Feature-encoded commands, clears/shader writes, other queues,
/// driver overhead and retained/resident bytes. Copying this snapshot grants no authority.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct WgpuNativeWorkStats {
    pub issued_buffer_write_calls: u64,
    pub issued_buffer_write_bytes: u64,
    pub issued_texture_write_calls: u64,
    pub issued_texture_write_bytes: u64,
    pub initialized_buffer_calls: u64,
    pub initialized_buffer_payload_bytes: u64,
    pub initialized_buffer_storage_bytes: u64,
    pub encoded_buffer_copy_commands: u64,
    pub encoded_buffer_copy_bytes: u64,
    pub encoded_texture_copy_commands: u64,
    pub encoded_texture_copy_bytes: u64,
}

impl WgpuNativeWorkStats {
    pub(crate) fn accumulate(&mut self, work: Self) {
        self.issued_buffer_write_calls = self
            .issued_buffer_write_calls
            .saturating_add(work.issued_buffer_write_calls);
        self.issued_buffer_write_bytes = self
            .issued_buffer_write_bytes
            .saturating_add(work.issued_buffer_write_bytes);
        self.issued_texture_write_calls = self
            .issued_texture_write_calls
            .saturating_add(work.issued_texture_write_calls);
        self.issued_texture_write_bytes = self
            .issued_texture_write_bytes
            .saturating_add(work.issued_texture_write_bytes);
        self.initialized_buffer_calls = self
            .initialized_buffer_calls
            .saturating_add(work.initialized_buffer_calls);
        self.initialized_buffer_payload_bytes = self
            .initialized_buffer_payload_bytes
            .saturating_add(work.initialized_buffer_payload_bytes);
        self.initialized_buffer_storage_bytes = self
            .initialized_buffer_storage_bytes
            .saturating_add(work.initialized_buffer_storage_bytes);
        self.encoded_buffer_copy_commands = self
            .encoded_buffer_copy_commands
            .saturating_add(work.encoded_buffer_copy_commands);
        self.encoded_buffer_copy_bytes = self
            .encoded_buffer_copy_bytes
            .saturating_add(work.encoded_buffer_copy_bytes);
        self.encoded_texture_copy_commands = self
            .encoded_texture_copy_commands
            .saturating_add(work.encoded_texture_copy_commands);
        self.encoded_texture_copy_bytes = self
            .encoded_texture_copy_bytes
            .saturating_add(work.encoded_texture_copy_bytes);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transaction_work_saturates_without_refunding_and_old_snapshots_stay_detached() {
        let work = WgpuNativeWorkStats {
            issued_buffer_write_calls: 1,
            issued_buffer_write_bytes: 2,
            issued_texture_write_calls: 3,
            issued_texture_write_bytes: 4,
            initialized_buffer_calls: 5,
            initialized_buffer_payload_bytes: 6,
            initialized_buffer_storage_bytes: 7,
            encoded_buffer_copy_commands: 8,
            encoded_buffer_copy_bytes: 9,
            encoded_texture_copy_commands: 10,
            encoded_texture_copy_bytes: 11,
        };
        let mut backend = crate::WgpuRenderBackend::default();
        backend.begin_frame_transaction(1, None);
        backend.record_native_work(work);
        let first = backend.frame_transaction_stats();
        backend.record_native_work(work);
        let doubled = backend.frame_transaction_stats().native_work();
        assert_eq!(first.native_work(), work);
        assert_eq!(
            doubled,
            WgpuNativeWorkStats {
                issued_buffer_write_calls: 2,
                issued_buffer_write_bytes: 4,
                issued_texture_write_calls: 6,
                issued_texture_write_bytes: 8,
                initialized_buffer_calls: 10,
                initialized_buffer_payload_bytes: 12,
                initialized_buffer_storage_bytes: 14,
                encoded_buffer_copy_commands: 16,
                encoded_buffer_copy_bytes: 18,
                encoded_texture_copy_commands: 20,
                encoded_texture_copy_bytes: 22,
            }
        );
        let mut saturated = work;
        for _ in 0..64 {
            let copy = saturated;
            saturated.accumulate(copy);
        }
        assert_eq!(
            saturated,
            WgpuNativeWorkStats {
                issued_buffer_write_calls: u64::MAX,
                issued_buffer_write_bytes: u64::MAX,
                issued_texture_write_calls: u64::MAX,
                issued_texture_write_bytes: u64::MAX,
                initialized_buffer_calls: u64::MAX,
                initialized_buffer_payload_bytes: u64::MAX,
                initialized_buffer_storage_bytes: u64::MAX,
                encoded_buffer_copy_commands: u64::MAX,
                encoded_buffer_copy_bytes: u64::MAX,
                encoded_texture_copy_commands: u64::MAX,
                encoded_texture_copy_bytes: u64::MAX,
            }
        );
        backend.begin_frame_transaction(2, Some(9));
        assert_eq!(
            backend.frame_transaction_stats().native_work(),
            WgpuNativeWorkStats::default()
        );
        assert_eq!(first.native_work(), work);
        assert_eq!(first.render_transaction_frame_index(), Some(1));
    }
}
