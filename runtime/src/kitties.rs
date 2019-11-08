use crate::mtp;
use crate::heap::{Compare, Heap};
use codec::{Decode, Encode};
use rstd::{result, cmp, vec::Vec};
use sr_primitives::traits::{Hash, Zero, SaturatedConversion};
use support::{decl_event, decl_module, decl_storage, dispatch::Result,
              ensure, StorageMap, StorageValue, traits::Currency};
use system::ensure_signed;
use runtime_io::*;

const ONE_MINUTE: u64 = 60_000;
const ONE_DAY: u64 = 86_400_000;
const BASE_YOUNG_FACTOR: u8 = 5;
const BASE_MATURITY_FACTOR: u8 = 10;
const BASE_OLDNESS_FACTOR: u8 = 5;

#[derive(PartialEq)]
#[cfg_attr(feature = "std", derive(Debug))]
enum LifeStage {
    Young,
    Maturity,
    Oldness,
    Invalid,
}

#[derive(Encode, Decode, Default, Clone, PartialEq)]
#[cfg_attr(feature = "std", derive(Debug))]
pub struct Kitty<Hash, Balance, Moment> {
    id: Hash,
    dna: Hash,
    price: Balance,
    gen: u64,
    lifetime: Lifetime<Moment>,
}

#[derive(Encode, Decode, Default, Clone, PartialEq)]
#[cfg_attr(feature = "std", derive(Debug))]
pub struct Lifetime<Moment> {
    birth_time: Moment,
    maturity_time: Moment,
    old_time: Moment,
    end_time: Moment,
}

#[derive(Encode, Decode, Default, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "std", derive(Debug))]
pub struct Lifespan<Hash, Moment> {
    kitty_id: Hash,
    end_time: Moment,
}

pub trait Trait: balances::Trait + mtp::Trait {
    type Event: From<Event<Self>> + Into<<Self as system::Trait>::Event>;
}

pub struct EndTimeCompare<T> (rstd::marker::PhantomData<(T)>);

impl<T: timestamp::Trait> Compare for EndTimeCompare<T> {
    type A = Lifespan<<T as system::Trait>::Hash, T::Moment>;
    fn closer_than(x: &Self::A, y: &Self::A) -> bool { x.end_time < y.end_time }
}

type LifespanHeap<T> = Heap<Lifespan<<T as system::Trait>::Hash, <T as timestamp::Trait>::Moment>,
    EndTimeCompare<T>, LifespanArray<T>>;

decl_event!(
    pub enum Event<T>
    where
        <T as system::Trait>::AccountId,
        <T as system::Trait>::Hash,
        <T as balances::Trait>::Balance
    {
        Created(AccountId, Hash),
        PriceSet(AccountId, Hash, Balance),
        Transferred(AccountId, AccountId, Hash),
        Bought(AccountId, AccountId, Hash, Balance),
    }
);

decl_storage! {
    trait Store for Module<T: Trait> as KittyStorage {
        Kitties get(kitty): map T::Hash => Kitty<T::Hash, T::Balance, T::Moment>;
        KittyOwner get(owner_of): map T::Hash => Option<T::AccountId>;

        AllKittiesArray get(kitty_by_index): map u64 => T::Hash;
        AllKittiesCount get(all_kitties_count): u64;
        AllKittiesIndex: map T::Hash => u64;

        OwnedKittiesArray get(kitty_of_owner_by_index): map (T::AccountId, u64) => T::Hash;
        OwnedKittiesCount get(owned_kitty_count): map T::AccountId => u64;
        OwnedKittiesIndex: map T::Hash => u64;

        // As a storage only use for LifespanHeap. Do not modify it directly.
        LifespanArray: Vec<Lifespan<T::Hash, T::Moment>>;

        Nonce: u64;
    }
}

decl_module! {
    pub struct Module<T: Trait> for enum Call where origin: T::Origin {

        fn deposit_event() = default;

        fn create_kitty(origin) -> Result {
            let sender = ensure_signed(origin)?;
            let nonce = <Nonce>::get();
            let random_hash = (<system::Module<T>>::random_seed(), &sender, nonce)
                .using_encoded(<T as system::Trait>::Hashing::hash);

            let mtp = <mtp::Module<T>>::median_time_past();
            let new_kitty = Kitty {
                id: random_hash,
                dna: random_hash,
                price: Zero::zero(),
                gen: 0,
                lifetime: Self::generate_lifetime(mtp, random_hash)?,
            };

            Self::mint(sender, random_hash, new_kitty)?;

            <Nonce>::mutate(|n| *n += 1);

            Ok(())
        }

        fn set_price(origin, kitty_id: T::Hash, new_price: T::Balance) -> Result {
            let sender = ensure_signed(origin)?;

            ensure!(<Kitties<T>>::exists(kitty_id), "This cat does not exist");

            let owner = Self::owner_of(kitty_id).ok_or("No owner for this kitty")?;
            ensure!(owner == sender, "You do not own this cat");

            let mtp = <mtp::Module<T>>::median_time_past();
            let mut kitty = Self::kitty(kitty_id);
            ensure!(Self::could_transfer(mtp, &kitty),
                "This cat is not in the life stage that can be transferred");

            kitty.price = new_price;

            <Kitties<T>>::insert(kitty_id, kitty);

            Self::deposit_event(RawEvent::PriceSet(sender, kitty_id, new_price));

            Ok(())
        }

        fn transfer(origin, to: T::AccountId, kitty_id: T::Hash) -> Result {
            let sender = ensure_signed(origin)?;

            let owner = Self::owner_of(kitty_id).ok_or("No owner for this kitty")?;
            ensure!(owner == sender, "You do not own this kitty");

            let mtp = <mtp::Module<T>>::median_time_past();
            let kitty = Self::kitty(kitty_id);
            ensure!(Self::could_transfer(mtp, &kitty),
                "This cat is not in the life stage that can be transferred");

            Self::transfer_from(sender, to, kitty_id)?;

            Ok(())
        }

        fn buy_kitty(origin, kitty_id: T::Hash, max_price: T::Balance) -> Result {
            let sender = ensure_signed(origin)?;

            ensure!(<Kitties<T>>::exists(kitty_id), "This cat does not exist");

            let owner = Self::owner_of(kitty_id).ok_or("No owner for this kitty")?;
            ensure!(owner != sender, "You can't buy your own cat");

            let mut kitty = Self::kitty(kitty_id);

            let kitty_price = kitty.price;
            ensure!(!kitty_price.is_zero(), "The cat you want to buy is not for sale");
            ensure!(kitty_price <= max_price, "The cat you want to buy costs more than your max price");

            <balances::Module<T> as Currency<_>>::transfer(&sender, &owner, kitty_price)?;

            Self::transfer_from(owner.clone(), sender.clone(), kitty_id)
                .expect("`owner` is shown to own the kitty; \
                `owner` must have greater than 0 kitties, so transfer cannot cause underflow; \
                `all_kitty_count` shares the same type as `owned_kitty_count` \
                and minting ensure there won't ever be more than `max()` kitties, \
                which means transfer cannot cause an overflow; \
                qed");

            kitty.price = Zero::zero();
            <Kitties<T>>::insert(kitty_id, kitty);

            Self::deposit_event(RawEvent::Bought(sender, owner, kitty_id, kitty_price));

            Ok(())
        }

        fn breed_kitty(origin, kitty_id_1: T::Hash, kitty_id_2: T::Hash) -> Result{
            let sender = ensure_signed(origin)?;

            ensure!(<Kitties<T>>::exists(kitty_id_1), "This cat 1 does not exist");
            ensure!(<Kitties<T>>::exists(kitty_id_2), "This cat 2 does not exist");

            let kitty_1 = Self::kitty(kitty_id_1);
            let kitty_2 = Self::kitty(kitty_id_2);

            let mtp = <mtp::Module<T>>::median_time_past();
            ensure!(Self::could_breed(mtp, &kitty_1),
                "This cat 1 is not in the life stage that can be breed");
            ensure!(Self::could_breed(mtp, &kitty_2),
                "This cat 2 is not in the life stage that can be breed");

            let nonce = <Nonce>::get();
            let random_hash = (<system::Module<T>>::random_seed(), &sender, nonce)
                .using_encoded(<T as system::Trait>::Hashing::hash);

            let mut final_dna = kitty_1.dna;
            for (i, dna_2_element) in kitty_2.dna.as_ref().iter().enumerate() {
                if random_hash.as_ref()[i] % 2 == 0 {
                    final_dna.as_mut()[i] = *dna_2_element;
                }
            }

            let new_kitty = Kitty {
                id: random_hash,
                dna: final_dna,
                price: Zero::zero(),
                gen: cmp::max(kitty_1.gen, kitty_2.gen) + 1,
                lifetime: Self::generate_lifetime(mtp, final_dna)?,
            };

            Self::mint(sender, random_hash, new_kitty)?;

            <Nonce>::mutate(|n| *n += 1);

            Ok(())
        }

        fn on_finalize(_n: T::BlockNumber) {
            let mtp = <mtp::Module<T>>::median_time_past();
            Self::remove_expired_kitties(mtp);
        }
    }
}

impl<T: Trait> Module<T> {
    fn generate_lifetime(mtp: T::Moment, dna: T::Hash) -> result::Result<Lifetime<T::Moment>, &'static str> {
        let birth_time = mtp.saturated_into::<u64>();
        let maturity_time = birth_time.checked_add(ONE_MINUTE * u64::from(BASE_YOUNG_FACTOR + dna.as_ref()[0]))
            .ok_or("Overflow calculating the childhood for a new kitty")?;
        let old_time = maturity_time.checked_add(ONE_DAY* u64::from(BASE_MATURITY_FACTOR + dna.as_ref()[1]))
            .ok_or("Overflow calculating the manhood for a new kitty")?;
        let end_time = old_time.checked_add(ONE_MINUTE * u64::from(BASE_OLDNESS_FACTOR + dna.as_ref()[2]))
            .ok_or("Overflow calculating the old age for a new kitty")?;

        let lifetime = Lifetime {
            birth_time: mtp,
            maturity_time: maturity_time.saturated_into(),
            old_time: old_time.saturated_into(),
            end_time: end_time.saturated_into(),
        };

        Ok(lifetime)
    }

    fn life_stage(mtp: T::Moment, lifetime: &Lifetime<T::Moment>) -> LifeStage {
        if mtp.cmp(&lifetime.birth_time) == cmp::Ordering::Less {
            LifeStage::Invalid
        } else if mtp.cmp(&lifetime.maturity_time) == cmp::Ordering::Less {
            LifeStage::Young
        } else if mtp.cmp(&lifetime.old_time) == cmp::Ordering::Less {
            LifeStage::Maturity
        } else if mtp.cmp(&lifetime.end_time) == cmp::Ordering::Less {
            LifeStage::Oldness
        } else {
            LifeStage::Invalid
        }
    }

    fn could_breed(mtp: T::Moment, kitty: &Kitty<T::Hash, T::Balance, T::Moment>) -> bool {
        Self::life_stage(mtp, &kitty.lifetime) == LifeStage::Maturity
    }

    fn could_transfer(mtp: T::Moment, kitty: &Kitty<T::Hash, T::Balance, T::Moment>) -> bool {
        match Self::life_stage(mtp, &kitty.lifetime) {
            LifeStage::Young => true,
            LifeStage::Maturity => true,
            _ => false
        }
    }

    fn mint(to: T::AccountId, kitty_id: T::Hash, new_kitty: Kitty<T::Hash, T::Balance, T::Moment>) -> Result {
        ensure!(!<KittyOwner<T>>::exists(kitty_id), "Kitty already exists");

        let owned_kitty_count = Self::owned_kitty_count(&to);

        let new_owned_kitty_count = owned_kitty_count.checked_add(1)
            .ok_or("Overflow adding a new kitty to account balance")?;

        let all_kitties_count = Self::all_kitties_count();

        let new_all_kitties_count = all_kitties_count.checked_add(1)
            .ok_or("Overflow adding a new kitty to total supply")?;

        <Kitties<T>>::insert(kitty_id, &new_kitty);
        <KittyOwner<T>>::insert(kitty_id, &to);

        <AllKittiesArray<T>>::insert(all_kitties_count, kitty_id);
        <AllKittiesCount>::put(new_all_kitties_count);
        <AllKittiesIndex<T>>::insert(kitty_id, all_kitties_count);

        <OwnedKittiesArray<T>>::insert((to.clone(), owned_kitty_count), kitty_id);
        <OwnedKittiesCount<T>>::insert(&to, new_owned_kitty_count);
        <OwnedKittiesIndex<T>>::insert(kitty_id, owned_kitty_count);

        <LifespanHeap<T>>::push(Lifespan {
            kitty_id,
            end_time: new_kitty.lifetime.end_time,
        });

        Self::deposit_event(RawEvent::Created(to, kitty_id));

        Ok(())
    }

    fn transfer_from(from: T::AccountId, to: T::AccountId, kitty_id: T::Hash) -> Result {
        let owner = Self::owner_of(kitty_id).ok_or("No owner for this kitty")?;

        ensure!(owner == from, "'from' account does not own this kitty");

        let owned_kitty_count_from = Self::owned_kitty_count(&from);
        let owned_kitty_count_to = Self::owned_kitty_count(&to);

        let new_owned_kitty_count_to = owned_kitty_count_to.checked_add(1)
            .ok_or("Transfer causes overflow of 'to' kitty balance")?;

        let new_owned_kitty_count_from = owned_kitty_count_from.checked_sub(1)
            .ok_or("Transfer causes underflow of 'from' kitty balance")?;

        let kitty_index = <OwnedKittiesIndex<T>>::get(kitty_id);
        if kitty_index != new_owned_kitty_count_from {
            let last_kitty_id = <OwnedKittiesArray<T>>::get((from.clone(), new_owned_kitty_count_from));
            <OwnedKittiesArray<T>>::insert((from.clone(), kitty_index), last_kitty_id);
            <OwnedKittiesIndex<T>>::insert(last_kitty_id, kitty_index);
        }

        <KittyOwner<T>>::insert(&kitty_id, &to);
        <OwnedKittiesIndex<T>>::insert(kitty_id, owned_kitty_count_to);

        <OwnedKittiesArray<T>>::remove((from.clone(), new_owned_kitty_count_from));
        <OwnedKittiesArray<T>>::insert((to.clone(), owned_kitty_count_to), kitty_id);

        <OwnedKittiesCount<T>>::insert(&from, new_owned_kitty_count_from);
        <OwnedKittiesCount<T>>::insert(&to, new_owned_kitty_count_to);

        Self::deposit_event(RawEvent::Transferred(from, to, kitty_id));

        Ok(())
    }

    fn remove_expired_kitties(mtp: T::Moment) {
        let stake = Lifespan {
            kitty_id: T::Hash::default(),
            end_time: mtp,
        };
        let expired_kitties = <LifespanHeap<T>>::pop_vec(&stake);
        for lifespan in expired_kitties {
            Self::burn_token(lifespan.kitty_id);
        }
    }

    fn burn_token(kitty_id: T::Hash) {
        // delete kitty
        let count = Self::all_kitties_count();
        if count == 0 {
            // print err and return
            runtime_io::print("burn_token(): There is no kitty.")
        }
        let last_kitty_index = count - 1;
        let last_kitty_id = Self::kitty_by_index(last_kitty_index);
        let kitty_index = <AllKittiesIndex<T>>::get(&kitty_id);
        <AllKittiesArray<T>>::insert(kitty_index, &last_kitty_id);
        <AllKittiesArray<T>>::remove(last_kitty_index);
        <AllKittiesIndex<T>>::insert(last_kitty_id, kitty_index);
        <AllKittiesIndex<T>>::remove(&kitty_id);
        AllKittiesCount::put(last_kitty_index);

        <Kitties<T>>::remove(kitty_id);

        // delete owner ship
        let owner = Self::owner_of(&kitty_id);
        if owner.is_none() {
            // print err and return
            runtime_io::print("burn_token(): No owner for this kitty")
        }
        let owner = owner.unwrap();
        let owned_count = Self::owned_kitty_count(&owner);
        if owned_count == 0 {
            // print err and return
            runtime_io::print("burn_token(): There is no ownership information")
        }
        let last_owned_index = owned_count - 1;
        let last_owned_id = Self::kitty_of_owner_by_index((owner.clone(), last_owned_index));
        let owned_index = <OwnedKittiesIndex<T>>::get(&kitty_id);
        <OwnedKittiesArray<T>>::insert((owner.clone(), owned_index), &last_owned_id);
        <OwnedKittiesArray<T>>::remove((owner.clone(), last_owned_index));
        <OwnedKittiesIndex<T>>::insert(last_owned_id, owned_index);
        <OwnedKittiesIndex<T>>::remove(&kitty_id);
        <OwnedKittiesCount<T>>::insert(owner, last_owned_index);

        <KittyOwner<T>>::remove(kitty_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use runtime_io::with_externalities;
    use primitives::{H256, Blake2Hasher};
    use support::{impl_outer_origin, assert_ok, parameter_types};
    use sr_primitives::{traits::{BlakeTwo256, IdentityLookup}, testing::Header};
    use sr_primitives::weights::Weight;
    use sr_primitives::Perbill;
    use timestamp;
    use std::str::FromStr;

    impl_outer_origin! {
      pub enum Origin for Test {}
    }

    // For testing the module, we construct most of a mock runtime. This means
    // first constructing a configuration type (`Test`) which `impl`s each of the
    // configuration traits of modules we want to use.
    #[derive(Clone, Eq, PartialEq)]
    pub struct Test;
    parameter_types! {
      pub const BlockHashCount: u64 = 250;
      pub const MaximumBlockWeight: Weight = 1024;
      pub const MaximumBlockLength: u32 = 2 * 1024;
      pub const AvailableBlockRatio: Perbill = Perbill::from_percent(75);
    }

    impl system::Trait for Test {
        type Origin = Origin;
        type Call = ();
        type Index = u64;
        type BlockNumber = u64;
        type Hash = H256;
        type Hashing = BlakeTwo256;
        type AccountId = u64;
        type Lookup = IdentityLookup<Self::AccountId>;
        type WeightMultiplierUpdate = ();
        type Header = Header;
        type Event = ();
        type BlockHashCount = BlockHashCount;
        type MaximumBlockWeight = MaximumBlockWeight;
        type MaximumBlockLength = MaximumBlockLength;
        type AvailableBlockRatio = AvailableBlockRatio;
        type Version = ();
    }

    impl balances::Trait for Test {
        type Balance = u64;
        type OnFreeBalanceZero = ();
        type OnNewAccount = ();
        type TransactionPayment = ();
        type TransferPayment = ();
        type DustRemoval = ();
        type Event = ();
        type ExistentialDeposit = ();
        type TransferFee = ();
        type CreationFee = ();
        type TransactionBaseFee = ();
        type TransactionByteFee = ();
        type WeightToFee = ();
    }

    impl timestamp::Trait for Test {
        type Moment = u64;
        type OnTimestampSet = ();
        type MinimumPeriod = ();
    }

    impl mtp::Trait for Test {}

    impl Trait for Test {
        type Event = ();
    }

    type TemplateModule = Module<Test>;

    // This function basically just builds a genesis storage key/value store according to
    // our desired mockup.
    fn new_test_ext() -> runtime_io::TestExternalities<Blake2Hasher> {
        system::GenesisConfig::default().build_storage::<Test>().unwrap().into()
    }

    #[test]
    fn generate_lifetime_test() {
        with_externalities(&mut new_test_ext(), || {
            let dna = H256::from_str(
                "0000000000000000000000000000000000000000000000000000000000000000"
            ).unwrap();
            let birth_time: u64 = 100;
            let maturity_time = birth_time + BASE_YOUNG_FACTOR as u64 * ONE_MINUTE;
            let old_time = maturity_time + BASE_MATURITY_FACTOR as u64 * ONE_DAY;
            let end_time = old_time + BASE_OLDNESS_FACTOR as u64 * ONE_MINUTE;
            assert_ok!(TemplateModule::generate_lifetime(100, dna),
                Lifetime{ birth_time, maturity_time, old_time, end_time, });

            let dna = H256::from_str(
                "0203040000000000000000000000000000000000000000000000000000000000"
            ).unwrap();
            let birth_time: u64 = 100;
            let maturity_time = birth_time + (BASE_YOUNG_FACTOR + 2) as u64 * ONE_MINUTE;
            let old_time = maturity_time + (BASE_MATURITY_FACTOR + 3) as u64 * ONE_DAY;
            let end_time = old_time + (BASE_OLDNESS_FACTOR + 4) as u64 * ONE_MINUTE;
            assert_ok!(TemplateModule::generate_lifetime(100, dna),
                Lifetime{ birth_time, maturity_time, old_time, end_time, });
        });
    }

    #[test]
    fn life_stage_test() {
        with_externalities(&mut new_test_ext(), || {
            let lifetime = Lifetime {
                birth_time: 100,
                maturity_time: 200,
                old_time: 300,
                end_time: 400,
            };
            assert_eq!(TemplateModule::life_stage(90, &lifetime), LifeStage::Invalid);
            assert_eq!(TemplateModule::life_stage(100, &lifetime), LifeStage::Young);
            assert_eq!(TemplateModule::life_stage(199, &lifetime), LifeStage::Young);
            assert_eq!(TemplateModule::life_stage(200, &lifetime), LifeStage::Maturity);
            assert_eq!(TemplateModule::life_stage(299, &lifetime), LifeStage::Maturity);
            assert_eq!(TemplateModule::life_stage(300, &lifetime), LifeStage::Oldness);
            assert_eq!(TemplateModule::life_stage(350, &lifetime), LifeStage::Oldness);
            assert_eq!(TemplateModule::life_stage(400, &lifetime), LifeStage::Invalid);
            assert_eq!(TemplateModule::life_stage(500, &lifetime), LifeStage::Invalid);
        });
    }

    #[test]
    fn life_stage_limit_test() {
        with_externalities(&mut new_test_ext(), || {
            let kitty = Kitty {
                id: H256::default(),
                dna: H256::default(),
                price: 0,
                gen: 0,
                lifetime: Lifetime {
                    birth_time: 100,
                    maturity_time: 200,
                    old_time: 300,
                    end_time: 400,
                },
            };

            assert_eq!(TemplateModule::could_breed(199, &kitty), false);
            assert_eq!(TemplateModule::could_breed(200, &kitty), true);
            assert_eq!(TemplateModule::could_breed(210, &kitty), true);
            assert_eq!(TemplateModule::could_breed(300, &kitty), false);
            assert_eq!(TemplateModule::could_breed(310, &kitty), false);

            assert_eq!(TemplateModule::could_transfer(99, &kitty), false);
            assert_eq!(TemplateModule::could_transfer(100, &kitty), true);
            assert_eq!(TemplateModule::could_transfer(110, &kitty), true);
            assert_eq!(TemplateModule::could_transfer(200, &kitty), true);
            assert_eq!(TemplateModule::could_transfer(210, &kitty), true);
            assert_eq!(TemplateModule::could_transfer(300, &kitty), false);
            assert_eq!(TemplateModule::could_transfer(400, &kitty), false);
        });
    }
}