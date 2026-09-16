use indexmap::IndexMap;
use serde_json::Value;
use std::hash::Hash;

pub trait Merge<R = Self> {
    fn merge(self, rhs: R) -> Self;
}

impl<T: Merge<T>> Merge<Option<T>> for T {
    fn merge(self, other_opt: Option<T>) -> Self {
        if let Some(other) = other_opt {
            self.merge(other)
        } else {
            self
        }
    }
}

impl<K: Eq + Hash, V: Merge<Option<V>>> Merge for IndexMap<K, V> {
    fn merge(self, mut rhs: Self) -> Self {
        let mut m = self
            .into_iter()
            .map(|(k, a)| {
                let v = a.merge(rhs.swap_remove(&k));

                (k, v)
            })
            .collect::<IndexMap<_, _>>();

        m.append(&mut rhs);

        m
    }
}

impl Merge for String {
    fn merge(self, rhs: Self) -> Self {
        format!("{self}{rhs}")
    }
}

impl Merge for Value {
    fn merge(self, rhs: Self) -> Self {
        rhs
    }
}

impl Merge for Vec<Value> {
    fn merge(mut self, mut rhs: Self) -> Self {
        self.append(&mut rhs);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn right_hand_value_wins_and_maps_merge_per_key() {
        let lhs: IndexMap<String, Value> =
            serde_json::from_value(json!({"a": 1, "b": [1]})).unwrap();
        let rhs: IndexMap<String, Value> =
            serde_json::from_value(json!({"b": [2], "c": true})).unwrap();

        let merged = lhs.merge(rhs);

        assert_eq!(
            serde_json::to_value(merged).unwrap(),
            json!({"a": 1, "b": [2], "c": true})
        );
    }

    #[test]
    fn value_vectors_append() {
        assert_eq!(
            vec![json!(1)].merge(vec![json!(2)]),
            vec![json!(1), json!(2)]
        );
    }
}
