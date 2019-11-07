use rstd::prelude::*;
use support::{decl_module, decl_storage, StorageValue};

pub trait Trait: timestamp::Trait {}

const MAX_TIMESTAMP_SAMPLES: usize = 11;

decl_storage! {
    trait Store for Module<T: Trait> as MTP {
        /// Stores the median time past calculated by the last 11 block.
        pub MedianTimePast get(median_time_past): T::Moment;
        /// Stores the timestamps of last 11 blocks.
        pub SampleTimestamps get(sample_timestamps): Vec<T::Moment>;
    }
}

decl_module! {
    pub struct Module<T: Trait> for enum Call where origin: T::Origin {
        fn on_finalize(_n: T::BlockNumber) {
            let now = <timestamp::Module<T>>::get();
            Self::calculate_mtp(now);
        }
    }
}

impl<T: Trait> Module<T> {
    fn calculate_mtp(time: T::Moment) {
        let mut samples = <SampleTimestamps<T>>::get();
        match samples.len() {
            MAX_TIMESTAMP_SAMPLES => {
                samples.remove(0);
                samples.push(time);
            }
            _ => samples = [time; MAX_TIMESTAMP_SAMPLES].to_vec(),
        }
        <SampleTimestamps<T>>::put(&samples);

        samples.sort();
        <MedianTimePast<T>>::put(samples[MAX_TIMESTAMP_SAMPLES / 2]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use runtime_io::with_externalities;
    use primitives::{H256, Blake2Hasher};
    use support::{impl_outer_origin, parameter_types};
    use sr_primitives::{traits::{BlakeTwo256, IdentityLookup}, testing::Header};
    use sr_primitives::weights::Weight;
    use sr_primitives::Perbill;
    use timestamp;

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

    impl timestamp::Trait for Test {
        type Moment = u64;
        type OnTimestampSet = ();
        type MinimumPeriod = ();
    }

    impl Trait for Test {
    }

    type TemplateModule = Module<Test>;

    // This function basically just builds a genesis storage key/value store according to
    // our desired mockup.
    fn new_test_ext() -> runtime_io::TestExternalities<Blake2Hasher> {
        system::GenesisConfig::default().build_storage::<Test>().unwrap().into()
    }

    #[test]
    fn mtp_test() {
        with_externalities(&mut new_test_ext(), || {
            // test initialization
            TemplateModule::calculate_mtp(100);
            assert_eq!(TemplateModule::median_time_past(), 100);
            assert_eq!(TemplateModule::sample_timestamps(), [100; MAX_TIMESTAMP_SAMPLES].to_vec());

            // test calculation
            let times = &[101,102,103,104,105,106,107,108,109,110,111];
            for time in times {
                TemplateModule::calculate_mtp(*time as u64);
            }
            assert_eq!(TemplateModule::median_time_past(), 106);
            assert_eq!(TemplateModule::sample_timestamps(), times.to_vec());

            let times = &[104,103,109,102,101,107,108,111,110,105,106];
            for time in times {
                TemplateModule::calculate_mtp(*time as u64);
            }
            assert_eq!(TemplateModule::median_time_past(), 106);
            assert_eq!(TemplateModule::sample_timestamps(), times.to_vec());
        });
    }
}
