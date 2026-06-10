use yrs::{Doc, StateVector, Update, Transact, ReadTxn};
use yrs::updates::decoder::Decode;
use yrs::updates::encoder::Encode;
use yrs::encoding::read::Error as DecodeError;
use yrs::error::UpdateError;

#[derive(thiserror::Error, Debug)]
pub enum SyncError {
    #[error("CRDT Decode error: {0}")]
    Decode(#[from] DecodeError),
    #[error("CRDT Update error: {0}")]
    Update(#[from] UpdateError),
}

pub struct CollaborationSession {
    doc: Doc,
}

impl CollaborationSession {
    /// Creates a new empty collaboration document session.
    pub fn new() -> Self {
        Self { doc: Doc::new() }
    }

    /// Applies concurrent client update binary data directly to the CRDT document.
    pub fn apply_update(&self, update: &[u8]) -> Result<(), SyncError> {
        let mut txn = self.doc.transact_mut();
        let update_decoded = Update::decode_v1(update)?;
        txn.apply_update(update_decoded)?;
        Ok(())
    }

    /// Generates the current state vector binary for synchronization handshakes.
    pub fn get_state_vector(&self) -> Vec<u8> {
        let txn = self.doc.transact();
        txn.state_vector().encode_v1()
    }

    /// Encodes the document state difference as update binary data based on the client's state vector.
    pub fn encode_state_as_update(&self, state_vector_bytes: &[u8]) -> Result<Vec<u8>, SyncError> {
        let sv = StateVector::decode_v1(state_vector_bytes)?;
        let txn = self.doc.transact();
        let diff = txn.encode_state_as_update_v1(&sv);
        Ok(diff)
    }
}

impl Default for CollaborationSession {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use yrs::{Text, GetString};

    #[test]
    fn test_sync_between_sessions() {
        let session_a = CollaborationSession::new();
        let session_b = CollaborationSession::new();

        // Write some text in session A
        {
            let text_a = session_a.doc.get_or_insert_text("content");
            let mut txn_a = session_a.doc.transact_mut();
            text_a.insert(&mut txn_a, 0, "Hello Frappe");
        }

        // Get difference update from A to sync with B
        let sv_b = session_b.get_state_vector();
        let update_from_a = session_a.encode_state_as_update(&sv_b).unwrap();

        // Apply update to B
        session_b.apply_update(&update_from_a).unwrap();

        // Verify text in B
        let text_b = session_b.doc.get_or_insert_text("content");
        let txn_b = session_b.doc.transact();
        let content_b = text_b.get_string(&txn_b);
        assert_eq!(content_b, "Hello Frappe");
    }
}
