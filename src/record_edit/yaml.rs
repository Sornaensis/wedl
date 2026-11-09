//! Bound expanded YAML before allocating a Value. Raw byte limits belong to
//! each caller; this shared visitor preserves the parser's ordinary YAML grammar.

/// Count each expanded value/key and every UTF-8 string byte, including aliases.
/// Retains no expanded values. The parser's alias-jump limit alone does not
/// bound repeated large scalars. Both callers keep the existing depth 128 bound.
pub(crate) fn preflight(
    input: &str,
    maximum_nodes: usize,
    maximum_scalar_bytes: usize,
) -> std::result::Result<(), serde_yaml::Error> {
    use serde::de::{DeserializeSeed, EnumAccess, MapAccess, SeqAccess, VariantAccess, Visitor};

    struct Budget {
        nodes: usize,
        scalar_bytes: usize,
        maximum_nodes: usize,
        maximum_scalar_bytes: usize,
    }
    struct Scan<'a> {
        budget: &'a mut Budget,
        depth: usize,
    }
    impl<'de> DeserializeSeed<'de> for Scan<'_> {
        type Value = ();

        fn deserialize<D: serde::Deserializer<'de>>(
            self,
            decoder: D,
        ) -> std::result::Result<(), D::Error> {
            self.budget.nodes += 1;
            if self.depth > 128 || self.budget.nodes > self.budget.maximum_nodes {
                return Err(serde::de::Error::custom(
                    "expanded YAML work bound exceeded",
                ));
            }
            decoder.deserialize_any(self)
        }
    }
    impl<'de> Visitor<'de> for Scan<'_> {
        type Value = ();

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("bounded YAML")
        }
        fn visit_bool<E: serde::de::Error>(self, _: bool) -> std::result::Result<(), E> {
            Ok(())
        }
        fn visit_i64<E: serde::de::Error>(self, _: i64) -> std::result::Result<(), E> {
            Ok(())
        }
        fn visit_u64<E: serde::de::Error>(self, _: u64) -> std::result::Result<(), E> {
            Ok(())
        }
        fn visit_f64<E: serde::de::Error>(self, _: f64) -> std::result::Result<(), E> {
            Ok(())
        }
        fn visit_unit<E: serde::de::Error>(self) -> std::result::Result<(), E> {
            Ok(())
        }
        fn visit_none<E: serde::de::Error>(self) -> std::result::Result<(), E> {
            Ok(())
        }
        fn visit_str<E: serde::de::Error>(self, value: &str) -> std::result::Result<(), E> {
            self.budget.scalar_bytes = self.budget.scalar_bytes.saturating_add(value.len());
            if self.budget.scalar_bytes > self.budget.maximum_scalar_bytes {
                return Err(serde::de::Error::custom(
                    "expanded YAML scalar byte bound exceeded",
                ));
            }
            Ok(())
        }
        fn visit_seq<A: SeqAccess<'de>>(self, mut values: A) -> std::result::Result<(), A::Error> {
            while values
                .next_element_seed(Scan {
                    budget: &mut *self.budget,
                    depth: self.depth + 1,
                })?
                .is_some()
            {}
            Ok(())
        }
        fn visit_map<A: MapAccess<'de>>(self, mut values: A) -> std::result::Result<(), A::Error> {
            while values
                .next_key_seed(Scan {
                    budget: &mut *self.budget,
                    depth: self.depth + 1,
                })?
                .is_some()
            {
                values.next_value_seed(Scan {
                    budget: &mut *self.budget,
                    depth: self.depth + 1,
                })?;
            }
            Ok(())
        }
        fn visit_enum<A: EnumAccess<'de>>(self, value: A) -> std::result::Result<(), A::Error> {
            let (_, contents) = value.variant_seed(Scan {
                budget: &mut *self.budget,
                depth: self.depth + 1,
            })?;
            contents.newtype_variant_seed(Scan {
                budget: self.budget,
                depth: self.depth + 1,
            })
        }
    }
    Scan {
        budget: &mut Budget {
            nodes: 0,
            scalar_bytes: 0,
            maximum_nodes,
            maximum_scalar_bytes,
        },
        depth: 0,
    }
    .deserialize(serde_yaml::Deserializer::from_str(input))
}
