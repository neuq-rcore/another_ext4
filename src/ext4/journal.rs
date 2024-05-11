use super::Ext4;

impl Ext4 {
    /// start transaction
    pub(super) fn ext4_trans_start(&self) {}

    /// stop transaction
    #[allow(unused)]
    pub(super) fn ext4_trans_abort(&self) {}
}
