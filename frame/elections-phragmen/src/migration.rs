use super::*;
use frame_support::{migration::{StorageKeyIterator, take_storage_item}, Twox64Concat};

pub fn migrate<T: Trait>() {
    sp_runtime::print("🕊️  Migrating Election Phragmen.");
    for (who, votes) in StorageKeyIterator
        ::<T::AccountId, Vec<T::AccountId>, Twox64Concat>
        ::new(b"PhragmenElection", b"VotesOf")
        .drain()
    {
        if let Some(stake) = take_storage_item::<_, BalanceOf<T>, Twox64Concat>(b"PhragmenElection", b"StakeOf", &who) {
            Voting::<T>::insert(who, (stake, votes));
        }
        sp_runtime::print("🕊️  Done Election Phragmen.");
    }
}