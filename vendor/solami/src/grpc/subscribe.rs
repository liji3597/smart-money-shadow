use std::collections::HashMap;

use crate::geyser::{
    CommitmentLevel, SubscribeRequest, SubscribeRequestAccountsDataSlice,
    SubscribeRequestFilterAccounts, SubscribeRequestFilterBlocks,
    SubscribeRequestFilterBlocksMeta, SubscribeRequestFilterEntry,
    SubscribeRequestFilterSlots, SubscribeRequestFilterTransactions,
};

pub struct SubscriptionBuilder {
    accounts: HashMap<String, SubscribeRequestFilterAccounts>,
    slots: HashMap<String, SubscribeRequestFilterSlots>,
    transactions: HashMap<String, SubscribeRequestFilterTransactions>,
    transactions_status: HashMap<String, SubscribeRequestFilterTransactions>,
    blocks: HashMap<String, SubscribeRequestFilterBlocks>,
    blocks_meta: HashMap<String, SubscribeRequestFilterBlocksMeta>,
    entry: HashMap<String, SubscribeRequestFilterEntry>,
    commitment: Option<i32>,
    accounts_data_slice: Vec<SubscribeRequestAccountsDataSlice>,
    from_slot: Option<u64>,
}

impl SubscriptionBuilder {
    pub fn new() -> Self {
        Self {
            accounts: HashMap::new(),
            slots: HashMap::new(),
            transactions: HashMap::new(),
            transactions_status: HashMap::new(),
            blocks: HashMap::new(),
            blocks_meta: HashMap::new(),
            entry: HashMap::new(),
            commitment: None,
            accounts_data_slice: vec![],
            from_slot: None,
        }
    }

    pub fn commitment(mut self, level: CommitmentLevel) -> Self {
        self.commitment = Some(level as i32);
        self
    }

    pub fn from_slot(mut self, slot: u64) -> Self {
        self.from_slot = Some(slot);
        self
    }

    pub fn accounts_data_slice(mut self, offset: u64, length: u64) -> Self {
        self.accounts_data_slice.push(SubscribeRequestAccountsDataSlice { offset, length });
        self
    }

    pub fn transactions(
        mut self,
        label: &str,
        filter: SubscribeRequestFilterTransactions,
    ) -> Self {
        self.transactions.insert(label.to_owned(), filter);
        self
    }

    pub fn transactions_status(
        mut self,
        label: &str,
        filter: SubscribeRequestFilterTransactions,
    ) -> Self {
        self.transactions_status.insert(label.to_owned(), filter);
        self
    }

    pub fn accounts(
        mut self,
        label: &str,
        filter: SubscribeRequestFilterAccounts,
    ) -> Self {
        self.accounts.insert(label.to_owned(), filter);
        self
    }

    pub fn slots(mut self, label: &str, filter: SubscribeRequestFilterSlots) -> Self {
        self.slots.insert(label.to_owned(), filter);
        self
    }

    pub fn blocks(mut self, label: &str, filter: SubscribeRequestFilterBlocks) -> Self {
        self.blocks.insert(label.to_owned(), filter);
        self
    }

    pub fn blocks_meta(mut self, label: &str) -> Self {
        self.blocks_meta.insert(label.to_owned(), SubscribeRequestFilterBlocksMeta {});
        self
    }

    pub fn entry(mut self, label: &str) -> Self {
        self.entry.insert(label.to_owned(), SubscribeRequestFilterEntry {});
        self
    }

    pub fn build(self) -> SubscribeRequest {
        SubscribeRequest {
            accounts: self.accounts,
            slots: self.slots,
            transactions: self.transactions,
            transactions_status: self.transactions_status,
            blocks: self.blocks,
            blocks_meta: self.blocks_meta,
            entry: self.entry,
            commitment: self.commitment,
            accounts_data_slice: self.accounts_data_slice,
            ping: None,
            from_slot: self.from_slot,
        }
    }
}

impl Default for SubscriptionBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_defaults_are_empty() {
        let req = SubscriptionBuilder::new().build();
        assert!(req.accounts.is_empty());
        assert!(req.slots.is_empty());
        assert!(req.transactions.is_empty());
        assert!(req.transactions_status.is_empty());
        assert!(req.blocks.is_empty());
        assert!(req.blocks_meta.is_empty());
        assert!(req.entry.is_empty());
        assert!(req.commitment.is_none());
        assert!(req.accounts_data_slice.is_empty());
        assert!(req.from_slot.is_none());
        assert!(req.ping.is_none());
    }

    #[test]
    fn default_matches_new() {
        let a = SubscriptionBuilder::default().build();
        let b = SubscriptionBuilder::new().build();
        assert_eq!(a.commitment, b.commitment);
        assert_eq!(a.from_slot, b.from_slot);
    }

    #[test]
    fn commitment_and_from_slot() {
        let req = SubscriptionBuilder::new()
            .commitment(CommitmentLevel::Processed)
            .from_slot(42)
            .build();
        assert_eq!(req.commitment, Some(CommitmentLevel::Processed as i32));
        assert_eq!(req.from_slot, Some(42));
    }

    #[test]
    fn accounts_data_slice_appends() {
        let req = SubscriptionBuilder::new()
            .accounts_data_slice(0, 10)
            .accounts_data_slice(100, 50)
            .build();
        assert_eq!(req.accounts_data_slice.len(), 2);
        assert_eq!(req.accounts_data_slice[0].offset, 0);
        assert_eq!(req.accounts_data_slice[0].length, 10);
        assert_eq!(req.accounts_data_slice[1].offset, 100);
        assert_eq!(req.accounts_data_slice[1].length, 50);
    }

    #[test]
    fn transactions_filter() {
        let filter = SubscribeRequestFilterTransactions {
            vote: Some(false),
            failed: Some(false),
            account_include: vec!["a".into()],
            account_exclude: vec![],
            account_required: vec![],
            signature: None,
        };
        let req = SubscriptionBuilder::new()
            .transactions("foo", filter.clone())
            .build();
        assert!(req.transactions.contains_key("foo"));
        assert_eq!(req.transactions["foo"].account_include, vec!["a".to_string()]);
    }

    #[test]
    fn transactions_status_filter() {
        let filter = SubscribeRequestFilterTransactions {
            vote: None,
            failed: None,
            account_include: vec![],
            account_exclude: vec![],
            account_required: vec![],
            signature: None,
        };
        let req = SubscriptionBuilder::new()
            .transactions_status("status", filter)
            .build();
        assert!(req.transactions_status.contains_key("status"));
    }

    #[test]
    fn accounts_filter() {
        let filter = SubscribeRequestFilterAccounts {
            account: vec!["a".into()],
            owner: vec!["o".into()],
            filters: vec![],
            nonempty_txn_signature: None,
        };
        let req = SubscriptionBuilder::new().accounts("acc", filter).build();
        assert!(req.accounts.contains_key("acc"));
    }

    #[test]
    fn slots_filter() {
        let filter = SubscribeRequestFilterSlots {
            filter_by_commitment: Some(true),
            interslot_updates: Some(false),
        };
        let req = SubscriptionBuilder::new().slots("s", filter).build();
        assert!(req.slots.contains_key("s"));
    }

    #[test]
    fn blocks_filter() {
        let filter = SubscribeRequestFilterBlocks {
            account_include: vec![],
            include_transactions: Some(true),
            include_accounts: Some(false),
            include_entries: Some(false),
        };
        let req = SubscriptionBuilder::new().blocks("b", filter).build();
        assert!(req.blocks.contains_key("b"));
    }

    #[test]
    fn blocks_meta_and_entry() {
        let req = SubscriptionBuilder::new()
            .blocks_meta("bm")
            .entry("ent")
            .build();
        assert!(req.blocks_meta.contains_key("bm"));
        assert!(req.entry.contains_key("ent"));
    }
}
