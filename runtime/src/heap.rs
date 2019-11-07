use rstd::vec::Vec;
use support::{Parameter, StorageValue};

pub trait Compare<T> {
    fn closer_than(x: &T, y: &T) -> bool;
}

pub struct Heap<T, C, S> (rstd::marker::PhantomData<(T, C, S)>);

impl<T, C, S> Heap<T, C, S>
    where T: Parameter,
          C: Compare<T>,
          S: StorageValue<Vec<T>, Query=Vec<T>>,
{
    pub fn push(item: T) {
        let mut store = S::get();
        Self::push_into_store(&mut store, item);
        S::put(store);
    }

    pub fn push_vec(items: Vec<T>) {
        let mut store = S::get();
        for item in items {
            Self::push_into_store(&mut store, item);
        }
        S::put(store);
    }

    pub fn pop() -> Option<T> {
        let mut store = S::get();
        let top = Self::pop_from_store(&mut store);
        S::put(store);
        top
    }

    pub fn pop_vec(stake: &T) -> Vec<T> {
        let mut store = S::get();
        let vec = Self::pop_by_stake(&mut store, stake);
        S::put(store);
        vec
    }

    fn push_into_store(store: &mut Vec<T>, item: T) {
        store.push(item);
        let last = store.len() - 1;
        Self::shift_up(store, last);
    }

    fn pop_by_stake(store: &mut Vec<T>, stack: &T) -> Vec<T> {
        let mut vec = Vec::new();
        let peek_top = store.get(0);
        match peek_top {
            None => vec,
            Some(peek_top) => {
                if C::closer_than(peek_top, stack) {
                    let top = Self::pop_from_store(store);
                    match top {
                        None => vec,
                        Some(top) => {
                            vec.push(top);
                            vec.append(&mut Self::pop_by_stake(store, stack));
                            vec
                        }
                    }
                } else {
                    vec
                }
            }
        }
    }

    fn pop_from_store(store: &mut Vec<T>) -> Option<T> {
        match store.len() {
            0 => None,
            1 => store.pop(),
            _ => {
                let last = store.len() - 1;
                store.swap(0, last);
                let top = store.pop();
                Self::shift_down(store, 0);
                top
            }
        }
    }

    fn parent_idx(child: usize) -> Option<usize> {
        match child {
            0 => None,
            1..=2 => Some(0),
            _ => {
                if child % 2 == 1 {
                    Some((child - 1) / 2)
                } else {
                    Some((child - 2) / 2)
                }
            }
        }
    }

    fn left_idx(store: &[T], parent: usize) -> Option<usize> {
        let left: usize = parent * 2 + 1;
        if left < store.len() {
            Some(left)
        } else {
            None
        }
    }

    fn right_idx(store: &[T], parent: usize) -> Option<usize> {
        let right: usize = parent * 2 + 2;
        if right < store.len() {
            Some(right)
        } else {
            None
        }
    }

    fn shift_up(store: &mut [T], idx: usize) {
        match Self::parent_idx(idx) {
            None => {}
            Some(par) => {
                if C::closer_than(&store[idx], &store[par]) {
                    store.swap(idx, par);
                    Self::shift_up(store, par);
                }
            }
        }
    }

    fn shift_down(store: &mut [T], idx: usize) {
        match Self::left_idx(store, idx) {
            None => {}
            Some(left) => {
                match Self::right_idx(store, idx) {
                    None => {
                        if C::closer_than(&store[left], &store[idx]) {
                            store.swap(idx, left);
                            Self::shift_down(store, left);
                        }
                    }
                    Some(right) => {
                        let closer =
                            if C::closer_than(&store[left], &store[right]) {
                                left
                            } else {
                                right
                            };
                        if C::closer_than(&store[closer], &store[idx]) {
                            store.swap(idx, closer);
                            Self::shift_down(store, closer);
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use runtime_io::with_externalities;
    use primitives::{H256, Blake2Hasher};
    use support::{decl_storage, decl_module, impl_outer_origin, assert_ok, parameter_types};
    use sr_primitives::{traits::{BlakeTwo256, IdentityLookup}, testing::Header};
    use sr_primitives::weights::Weight;
    use sr_primitives::Perbill;

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
        type Header = Header;
        type WeightMultiplierUpdate = ();
        type Event = ();
        type BlockHashCount = BlockHashCount;
        type MaximumBlockWeight = MaximumBlockWeight;
        type MaximumBlockLength = MaximumBlockLength;
        type AvailableBlockRatio = AvailableBlockRatio;
        type Version = ();
    }

    pub trait Trait: system::Trait {}

    impl Trait for Test {}

    decl_storage! {
        trait Store for Module<T: Trait> as Test {
            pub HeapStore get(heap_store): Vec<i32>;
        }
    }

    decl_module! { pub struct Module<T: Trait> for enum Call where origin: T::Origin { } }

    type TemplateModule = Module<Test>;

    // This function basically just builds a genesis storage key/value store according to
    // our desired mockup.
    fn new_test_ext() -> runtime_io::TestExternalities<Blake2Hasher> {
        system::GenesisConfig::default().build_storage::<Test>().unwrap().into()
    }

    struct TestCompare {}

    impl<A: Ord> Compare<A> for TestCompare {
        fn closer_than(x: &A, y: &A) -> bool { x > y }
    }

    type MaxHeap = Heap<i32, TestCompare, HeapStore>;

    #[test]
    fn it_works_for_default_value() {
        with_externalities(&mut new_test_ext(), || {
            assert_eq!(TemplateModule::heap_store(), [0; 0].to_vec());
            <HeapStore>::put([1, 2, 3].to_vec());
            assert_eq!(TemplateModule::heap_store(), [1, 2, 3].to_vec());
        });
    }

    #[test]
    fn parent_idx_test() {
        assert_eq!(MaxHeap::parent_idx(0), None);
        assert_eq!(MaxHeap::parent_idx(1), Some(0));
        assert_eq!(MaxHeap::parent_idx(2), Some(0));
        assert_eq!(MaxHeap::parent_idx(3), Some(1));
        assert_eq!(MaxHeap::parent_idx(4), Some(1));
        assert_eq!(MaxHeap::parent_idx(5), Some(2));
        assert_eq!(MaxHeap::parent_idx(6), Some(2));
        assert_eq!(MaxHeap::parent_idx(10), Some(4));
    }

    #[test]
    fn left_idx_test() {
        let store = &[10, 20, 30, 40, 50, 60, 70];
        assert_eq!(MaxHeap::left_idx(store, 0), Some(1));
        assert_eq!(MaxHeap::left_idx(store, 1), Some(3));
        assert_eq!(MaxHeap::left_idx(store, 2), Some(5));
        assert_eq!(MaxHeap::left_idx(store, 3), None);
        assert_eq!(MaxHeap::left_idx(store, 4), None);
    }

    #[test]
    fn right_idx_test() {
        let store = &[10, 20, 30, 40, 50, 60, 70];
        assert_eq!(MaxHeap::right_idx(store, 0), Some(2));
        assert_eq!(MaxHeap::right_idx(store, 1), Some(4));
        assert_eq!(MaxHeap::right_idx(store, 2), Some(6));
        assert_eq!(MaxHeap::right_idx(store, 3), None);
    }

    #[test]
    fn shift_up_test() {
        let store = &mut [10];
        MaxHeap::shift_up(store, 0);
        assert_eq!(store, &[10]);
        let store = &mut [10, 20];
        MaxHeap::shift_up(store, 1);
        assert_eq!(store, &[20, 10]);
        let store = &mut [10, 20, 30];
        MaxHeap::shift_up(store, 2);
        assert_eq!(store, &[30, 20, 10]);
        let store = &mut [10, 20, 30, 40];
        MaxHeap::shift_up(store, 3);
        assert_eq!(store, &[40, 10, 30, 20]);
        let store = &mut [10, 20, 30, 40, 50];
        MaxHeap::shift_up(store, 4);
        assert_eq!(store, &[50, 10, 30, 40, 20]);
        let store = &mut [10, 20, 30, 40, 50, 60];
        MaxHeap::shift_up(store, 5);
        assert_eq!(store, &[60, 20, 10, 40, 50, 30]);
    }

    #[test]
    fn shift_down_test() {
        let store = &mut [10];
        MaxHeap::shift_down(store, 0);
        assert_eq!(store, &[10]);
        let store = &mut [10, 20];
        MaxHeap::shift_down(store, 0);
        assert_eq!(store, &[20, 10]);
        let store = &mut [10, 20, 30];
        MaxHeap::shift_down(store, 0);
        assert_eq!(store, &[30, 20, 10]);
        let store = &mut [10, 20, 30, 40, 50, 60];
        MaxHeap::shift_down(store, 0);
        assert_eq!(store, &[30, 20, 60, 40, 50, 10]);
    }

    #[test]
    fn push_test() {
        with_externalities(&mut new_test_ext(), || {
            <HeapStore>::put([0; 0].to_vec());
            MaxHeap::push(10);
            assert_eq!(TemplateModule::heap_store(), [10].to_vec());
            MaxHeap::push(20);
            assert_eq!(TemplateModule::heap_store(), [20, 10].to_vec());
            MaxHeap::push(30);
            assert_eq!(TemplateModule::heap_store(), [30, 10, 20].to_vec());
            MaxHeap::push(40);
            MaxHeap::push(50);
            assert_eq!(TemplateModule::heap_store(), [50, 40, 20, 10, 30].to_vec());
        });
    }

    #[test]
    fn push_vec_test() {
        with_externalities(&mut new_test_ext(), || {
            <HeapStore>::put([0; 0].to_vec());
            MaxHeap::push_vec([10].to_vec());
            assert_eq!(TemplateModule::heap_store(), [10].to_vec());
            MaxHeap::push_vec([20, 30, 40, 50].to_vec());
            assert_eq!(TemplateModule::heap_store(), [50, 40, 20, 10, 30].to_vec());
        });
    }

    #[test]
    fn pop_test() {
        with_externalities(&mut new_test_ext(), || {
            <HeapStore>::put([0; 0].to_vec());
            assert_eq!(MaxHeap::pop(), None);
            <HeapStore>::put([10].to_vec());
            assert_eq!(MaxHeap::pop(), Some(10));
            <HeapStore>::put([50, 40, 20, 10, 30].to_vec());
            assert_eq!(MaxHeap::pop(), Some(50));
            assert_eq!(MaxHeap::pop(), Some(40));
            assert_eq!(MaxHeap::pop(), Some(30));
            assert_eq!(MaxHeap::pop(), Some(20));
            assert_eq!(MaxHeap::pop(), Some(10));
            assert_eq!(MaxHeap::pop(), None);
        });
    }

    #[test]
    fn pop_vec_test() {
        with_externalities(&mut new_test_ext(), || {
            <HeapStore>::put([0; 0].to_vec());
            assert_eq!(MaxHeap::pop_vec(&0), [0; 0].to_vec());
            assert_eq!(MaxHeap::pop_vec(&1), [0; 0].to_vec());
            <HeapStore>::put([10].to_vec());
            assert_eq!(MaxHeap::pop_vec(&10), [0; 0].to_vec());
            assert_eq!(MaxHeap::pop_vec(&0), [10].to_vec());
            <HeapStore>::put([50, 40, 20, 10, 30].to_vec());
            assert_eq!(MaxHeap::pop_vec(&35), [50, 40].to_vec());
            assert_eq!(MaxHeap::pop(), Some(30));
            assert_eq!(MaxHeap::pop_vec(&5), [20, 10].to_vec());
            assert_eq!(MaxHeap::pop_vec(&0), [0; 0].to_vec());
        });
    }
}
