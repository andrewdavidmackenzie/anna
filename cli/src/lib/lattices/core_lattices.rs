use std::collections::HashSet;
use std::hash::Hash;
use std::iter::Extend;
use std::ops::{Add, Sub};

trait Lattice {
    type A;

    fn do_merge(&mut self, _: &Self);
}

#[derive(Default)]
struct BoolLattice(bool);

impl Lattice for BoolLattice {
    type A = bool;

    fn do_merge(&mut self, l: &BoolLattice) {
        self.0 |= l.0;
    }
}

#[derive(Default, Clone, PartialEq, Debug)]
struct MaxLattice<T>(T)
where
    T: PartialOrd + PartialEq + Clone;

impl<T: Add<Output = T>> Add for MaxLattice<T>
where
    T: PartialOrd + PartialEq + Clone,
{
    type Output = Self;

    fn add(self, other: Self) -> Self::Output {
        Self(self.0 + other.0)
    }
}

impl<T: Sub<Output = T>> Sub for MaxLattice<T>
where
    T: PartialOrd + PartialEq + Clone,
{
    type Output = Self;

    fn sub(self, other: Self) -> Self::Output {
        Self(self.0 - other.0)
    }
}

impl<T> Lattice for MaxLattice<T>
where
    T: PartialOrd + PartialEq + Clone,
{
    type A = T;

    fn do_merge(&mut self, l: &MaxLattice<T>) {
        if self.0 < l.0 {
            self.0 = l.0.clone()
        }
    }
}

/// A `SetLattice` containing a set of elements of type `T`
#[derive(Default, Debug)]
struct SetLattice<T>(HashSet<T>);

impl<T> Lattice for SetLattice<T>
where
    T: Eq + Hash + Clone,
{
    type A = T;

    fn do_merge(&mut self, l: &SetLattice<T>) {
        let set = &mut self.0;
        let other_set = &l.0;
        set.extend(other_set.into_iter().map(Clone::clone));
    }
}

impl<T> PartialEq for SetLattice<T>
where
    T: Eq + Hash,
{
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl<T> SetLattice<T>
where
    T: Eq + Hash + Clone,
{
    /// Create a new, empty, `SetLattice`
    pub fn new() -> Self {
        SetLattice(HashSet::new())
    }

    /// Return the number of elements in the `SetLattice` as a `MaxLattice<usize>`
    pub fn size(&self) -> MaxLattice<usize> {
        MaxLattice(self.0.len())
    }

    /// Insert a new element into the `SetLattice`
    pub fn insert(&mut self, l: T) {
        let set = &mut self.0;
        set.insert(l);
    }

    /// Calculate a new `SetLattice` that is the intersection of `Self` with another `SetLattice`
    pub fn intersect(&self, l: &SetLattice<T>) -> SetLattice<T> {
        let other_set = &l.0;
        let intersection = self.0.intersection(other_set);
        let mut new_set: HashSet<T> = HashSet::new();
        for element in intersection {
            new_set.insert(element.clone());
        }
        Self(new_set)
    }

    // pub fn project(&self, function: FnOnce(&E) -> bool) -> Self {
    //     let sub_set = self.0.map(|e| {
    //         if function(e) {
    //             e
    //         }
    //     });
    //
    //     Set
    // }
}

// template <typename T>
// class OrderedSetLattice : public Lattice<ordered_set<T>> {
//  protected:
//   void do_merge(const ordered_set<T> &e) {
//     for (const T &elem : e) {
//       this->element.insert(elem);
//     }
//   }
//
//  public:
//   OrderedSetLattice() : Lattice<ordered_set<T>>(ordered_set<T>()) {}
//
//   OrderedSetLattice(const ordered_set<T> &e) : Lattice<ordered_set<T>>(e) {}
//
//   MaxLattice<unsigned> size() const { return this->element.size(); }
//
//   void insert(T e) { this->element.insert(std::move(e)); }
//
//   OrderedSetLattice<T> intersect(ordered_set<T> s) const {
//     ordered_set<T> res;
//
//     for (const T &that_elem : s) {
//       for (const T &this_elem : this->element) {
//         if (this_elem == that_elem) res.insert(this_elem);
//       }
//     }
//
//     return OrderedSetLattice<T>(res);
//   }
//
//   OrderedSetLattice<T> project(bool (*f)(T)) const {
//     ordered_set<T> res;
//
//     for (const T &elem : this->element) {
//       if (f(elem)) res.insert(elem);
//     }
//
//     return OrderedSetLattice<T>(res);
//   }
// };
//
// template <typename K, typename V>
// class MapLattice : public Lattice<map<K, V>> {
//  protected:
//   void insert_pair(const K &k, const V &v) {
//     auto search = this->element.find(k);
//     if (search != this->element.end()) {
//       static_cast<V *>(&(search->second))->merge(v);
//     } else {
//       // need to copy v since we will be "growing" it within the lattice
//       V new_v = v;
//       this->element.emplace(k, new_v);
//     }
//   }
//
//   void do_merge(const map<K, V> &m) {
//     for (const auto &pair : m) {
//       this->insert_pair(pair.first, pair.second);
//     }
//   }
//
//  public:
//   MapLattice() : Lattice<map<K, V>>(map<K, V>()) {}
//   MapLattice(const map<K, V> &m) : Lattice<map<K, V>>(m) {}
//   MaxLattice<unsigned> size() const { return this->element.size(); }
//
//   MapLattice<K, V> intersect(MapLattice<K, V> other) const {
//     MapLattice<K, V> res;
//     map<K, V> m = other.reveal();
//
//     for (const auto &pair : m) {
//       if (this->contains(pair.first).reveal()) {
//         res.insert_pair(pair.first, this->at(pair.first));
//         res.insert_pair(pair.first, pair.second);
//       }
//     }
//
//     return res;
//   }
//
//   MapLattice<K, V> project(bool (*f)(V)) const {
//     map<K, V> res;
//     for (const auto &pair : this->element) {
//       if (f(pair.second)) res.emplace(pair.first, pair.second);
//     }
//     return MapLattice<K, V>(res);
//   }
//
//   BoolLattice contains(K k) const {
//     auto it = this->element.find(k);
//     if (it == this->element.end())
//       return BoolLattice(false);
//     else
//       return BoolLattice(true);
//   }
//
//   SetLattice<K> key_set() const {
//     set<K> res;
//     for (const auto &pair : this->element) {
//       res.insert(pair.first);
//     }
//     return SetLattice<K>(res);
//   }
//
//   V &at(K k) { return this->element[k]; }
//
//   void remove(K k) {
//     auto it = this->element.find(k);
//     if (it != this->element.end()) this->element.erase(k);
//   }
//
//   void insert(const K &k, const V &v) { this->insert_pair(k, v); }
// };

#[cfg(test)]
mod test {
    use crate::lattices::core_lattices::{BoolLattice, Lattice, MaxLattice, SetLattice};
    use std::collections::HashSet;

    #[test]
    fn default_bool_lattice() {
        let bool_lattice = BoolLattice::default();
        assert_eq!(bool_lattice.0, false);
    }

    #[test]
    fn create_false_bool_lattice() {
        let bool_lattice = BoolLattice(false);
        assert_eq!(bool_lattice.0, false);
    }

    #[test]
    fn create_true_bool_lattice() {
        let bool_lattice = BoolLattice(true);
        assert_eq!(bool_lattice.0, true);
    }

    #[test]
    fn merge_true_false_bool_lattice() {
        let mut bool_lattice = BoolLattice(false);
        bool_lattice.do_merge(&BoolLattice(true));
        assert_eq!(bool_lattice.0, true)
    }

    #[test]
    fn default_max_u32_lattice() {
        let lattice = MaxLattice::<u32>::default();
        assert_eq!(lattice.0, 0);
    }

    #[test]
    fn merge_max_u32_lattice() {
        let mut low_lattice = MaxLattice::<u32>(1);
        let high_lattice = MaxLattice::<u32>(42);
        low_lattice.do_merge(&high_lattice);
        assert_eq!(low_lattice.0, 42)
    }

    #[test]
    fn add_max_u32_lattice() {
        let low_lattice = MaxLattice::<u32>(1);
        let high_lattice = MaxLattice::<u32>(42);
        assert_eq!(low_lattice + high_lattice, MaxLattice::<u32>(43))
    }

    #[test]
    fn sub_max_u32_lattice() {
        let low_lattice = MaxLattice::<u32>(1);
        let high_lattice = MaxLattice::<u32>(42);
        assert_eq!(high_lattice - low_lattice, MaxLattice::<u32>(41))
    }

    #[test]
    fn sub_max_u64_lattice() {
        let low_lattice = MaxLattice::<u64>(100);
        let high_lattice = MaxLattice::<u64>(142);
        assert_eq!(high_lattice - low_lattice, MaxLattice::<u64>(42))
    }

    #[test]
    fn size_of_empty_set() {
        // let set: HashSet<usize> = HashSet::new();
        let set_lattice: SetLattice<usize> = SetLattice::new();
        assert_eq!(set_lattice.size(), MaxLattice(0));
    }

    #[test]
    fn size_of_set() {
        let mut set: HashSet<usize> = HashSet::new();
        set.insert(1);
        set.insert(42);
        let set_lattice = SetLattice(set);
        assert_eq!(set_lattice.size(), MaxLattice(2));
    }

    #[test]
    fn insert_to_set() {
        // let set: HashSet<usize> = HashSet::new();
        let mut set_lattice: SetLattice<usize> = SetLattice::new();
        set_lattice.insert(1);
        set_lattice.insert(42);
        assert_eq!(set_lattice.size(), MaxLattice(2));
    }

    #[test]
    fn merge_two_sets() {
        // let set1: HashSet<usize> = HashSet::new();
        let mut set_lattice1: SetLattice<usize> = SetLattice::new();
        // let mut set_lattice1 = SetLattice(set1);
        set_lattice1.insert(1);
        set_lattice1.insert(42);

        let set2: HashSet<usize> = HashSet::new();
        let mut set_lattice2 = SetLattice(set2);
        set_lattice2.insert(3);
        set_lattice2.insert(100);

        set_lattice1.do_merge(&set_lattice2);

        assert_eq!(set_lattice1.size(), MaxLattice(4));
    }

    #[test]
    fn merge_two_intersecting_sets() {
        // let set1: HashSet<usize> = HashSet::new();
        // let mut set_lattice1 = SetLattice(set1);
        let mut set_lattice1: SetLattice<usize> = SetLattice::new();
        set_lattice1.insert(1);
        set_lattice1.insert(42);

        // let set2: HashSet<usize> = HashSet::new();
        // let mut set_lattice2 = SetLattice(set2);
        let mut set_lattice2: SetLattice<usize> = SetLattice::new();
        set_lattice2.insert(1);
        set_lattice2.insert(100);

        set_lattice1.do_merge(&set_lattice2);

        assert_eq!(set_lattice1.size(), MaxLattice(3));
    }

    #[test]
    fn intersection_of_two_sets() {
        // let set1: HashSet<usize> = HashSet::new();
        // let mut set_lattice1 = SetLattice(set1);
        let mut set_lattice1: SetLattice<usize> = SetLattice::new();
        set_lattice1.insert(1);
        set_lattice1.insert(42);

        // let set2: HashSet<usize> = HashSet::new();
        // let mut set_lattice2 = SetLattice(set2);
        let mut set_lattice2: SetLattice<usize> = SetLattice::new();
        set_lattice2.insert(1);
        set_lattice2.insert(100);

        assert_eq!(set_lattice1.intersect(&set_lattice2).size(), MaxLattice(1));
    }

    #[test]
    fn equality_of_two_sets() {
        // let set1: HashSet<usize> = HashSet::new();
        // let mut set_lattice1 = SetLattice(set1);
        let mut set_lattice1: SetLattice<usize> = SetLattice::new();
        set_lattice1.insert(1);
        set_lattice1.insert(42);

        // let set2: HashSet<usize> = HashSet::new();
        // let mut set_lattice2 = SetLattice(set2);
        let mut set_lattice2: SetLattice<usize> = SetLattice::new();
        set_lattice2.insert(1);
        set_lattice2.insert(42);

        assert_eq!(set_lattice1, set_lattice2);
    }
}
