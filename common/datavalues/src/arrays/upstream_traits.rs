// Copyright 2020 Datafuse Labs.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Implementations of upstream traits for DFPrimitiveArray<T>
use std::borrow::Borrow;
use std::borrow::Cow;
use std::iter::FromIterator;

use common_arrow::arrow::array::*;

use super::get_list_builder;
use crate::prelude::*;
use crate::series::Series;
use crate::utils::get_iter_capacity;

/// FromIterator trait

impl<T> FromIterator<Option<T>> for DFPrimitiveArray<T>
where T: DFPrimitiveType
{
    fn from_iter<I: IntoIterator<Item = Option<T>>>(iter: I) -> Self {
        let iter = iter.into_iter();

        let arr: PrimitiveArray<T> = match iter.size_hint() {
            (a, Some(b)) if a == b => unsafe {
                let arr = PrimitiveArray::from_trusted_len_iter_unchecked(iter);
                assert_eq!(arr.len(), a);
                arr
            },
            _ => iter.collect(),
        };
        arr.into()
    }
}

impl<T> FromIterator<T> for DFPrimitiveArray<T>
where T: DFPrimitiveType
{
    // We use AlignedVec because it is way faster than Arrows builder. We can do this because we
    // know we don't have null values.
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        let av = iter.into_iter().collect::<AlignedVec<T>>();
        DFPrimitiveArray::<T>::new_from_aligned_vec(av)
    }
}

impl FromIterator<Option<bool>> for DFBooleanArray {
    fn from_iter<I: IntoIterator<Item = Option<bool>>>(iter: I) -> Self {
        let arr = BooleanArray::from_iter(iter);
        arr.into()
    }
}

impl FromIterator<bool> for DFBooleanArray {
    fn from_iter<I: IntoIterator<Item = bool>>(iter: I) -> Self {
        // 2021-02-07: this was ~70% faster than with the builder, even with the extra Option<T> added.
        let arr: BooleanArray = iter.into_iter().map(Some).collect();

        arr.into()
    }
}

// FromIterator for StringType variants.Array
impl<Ptr> FromIterator<Option<Ptr>> for DFStringArray
where Ptr: AsRef<[u8]>
{
    fn from_iter<I: IntoIterator<Item = Option<Ptr>>>(iter: I) -> Self {
        let iter = iter.into_iter();
        let arr: LargeBinaryArray = match iter.size_hint() {
            (a, Some(b)) if a == b => {
                // 2021-02-07: ~40% faster than builder.
                unsafe {
                    let arr = LargeBinaryArray::from_trusted_len_iter_unchecked(iter);
                    assert_eq!(arr.len(), a);
                    arr
                }
            }
            _ => iter.collect(),
        };

        arr.into()
    }
}

/// Local AsRef<T> trait to circumvent the orphan rule.
pub trait DFAsRef<T: ?Sized>: AsRef<T> {}

impl DFAsRef<[u8]> for Vec<u8> {}
impl DFAsRef<[u8]> for &[u8] {}
// &["foo", "bar"]
impl DFAsRef<[u8]> for &&[u8] {}
impl<'a> DFAsRef<[u8]> for Cow<'a, [u8]> {}

impl<Ptr> FromIterator<Ptr> for DFStringArray
where Ptr: DFAsRef<[u8]>
{
    fn from_iter<I: IntoIterator<Item = Ptr>>(iter: I) -> Self {
        let arr = LargeBinaryArray::from_iter_values(iter.into_iter());
        arr.into()
    }
}

/// From trait
impl<'a> From<&'a DFStringArray> for Vec<Option<&'a [u8]>> {
    fn from(ca: &'a DFStringArray) -> Self {
        ca.inner().iter().collect()
    }
}

impl From<DFStringArray> for Vec<Option<Vec<u8>>> {
    fn from(ca: DFStringArray) -> Self {
        ca.inner()
            .iter()
            .map(|opt| opt.map(|s| s.to_vec()))
            .collect()
    }
}

impl<'a> From<&'a DFBooleanArray> for Vec<Option<bool>> {
    fn from(ca: &'a DFBooleanArray) -> Self {
        ca.collect_values()
    }
}

impl From<DFBooleanArray> for Vec<Option<bool>> {
    fn from(ca: DFBooleanArray) -> Self {
        ca.collect_values()
    }
}

impl<'a, T> From<&'a DFPrimitiveArray<T>> for Vec<Option<T>>
where T: DFPrimitiveType
{
    fn from(ca: &'a DFPrimitiveArray<T>) -> Self {
        ca.collect_values()
    }
}

impl<Ptr> FromIterator<Ptr> for DFListArray
where Ptr: Borrow<Series>
{
    fn from_iter<I: IntoIterator<Item = Ptr>>(iter: I) -> Self {
        let mut it = iter.into_iter();
        let capacity = get_iter_capacity(&it);

        // first take one to get the dtype. We panic if we have an empty iterator
        let v = it.next().unwrap();
        // We don't know the needed capacity. We arbitrarily choose an average of 5 elements per series.
        let mut builder = get_list_builder(v.borrow().data_type(), capacity * 5, capacity);

        builder.append_series(v.borrow());
        for s in it {
            builder.append_series(s.borrow());
        }
        builder.finish()
    }
}

impl<Ptr> FromIterator<Option<Ptr>> for DFListArray
where Ptr: Borrow<Series>
{
    fn from_iter<I: IntoIterator<Item = Option<Ptr>>>(iter: I) -> Self {
        let mut it = iter.into_iter();
        let owned_v;
        let mut cnt = 0;

        loop {
            let opt_v = it.next();

            match opt_v {
                Some(opt_v) => match opt_v {
                    Some(val) => {
                        owned_v = val;
                        break;
                    }
                    None => cnt += 1,
                },
                // end of iterator
                None => {
                    // type is not known
                    panic!("Type of Series cannot be determined as they are all null")
                }
            }
        }
        let v = owned_v.borrow();
        let capacity = get_iter_capacity(&it);
        let mut builder = get_list_builder(v.data_type(), capacity * 5, capacity);

        // first fill all None's we encountered
        while cnt > 0 {
            builder.append_opt_series(None);
            cnt -= 1;
        }

        // now the first non None
        builder.append_series(v);

        // now we have added all Nones, we can consume the rest of the iterator.
        for opt_s in it {
            match opt_s {
                Some(s) => builder.append_series(s.borrow()),
                None => builder.append_null(),
            }
        }

        builder.finish()
    }
}
