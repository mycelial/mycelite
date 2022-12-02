//! Journal Data Format Deserializer

use crate::error::Error;
use block::Block;
use serde::{
    de::{self, Deserializer, Visitor},
    Deserialize,
};
use std::io::Read;

struct De<R> {
    reader: R,
}

impl<R: Read> De<R> {
    fn from_reader(reader: R) -> Self {
        Self { reader }
    }
}

impl<'de, 'a, R> Deserializer<'de> for &'a mut De<R>
where
    R: Read,
{
    type Error = Error;

    fn deserialize_any<V>(self, _v: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        Err(Self::Error::Unsupported)
    }

    fn deserialize_bool<V>(self, v: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let mut buf = [0_u8; 1];
        self.reader.read_exact(&mut buf)?;
        v.visit_bool(buf[0] == 1)
    }

    fn deserialize_i8<V>(self, v: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let mut buf = [0; 1];
        self.reader.read_exact(&mut buf)?;
        v.visit_i8(i8::from_be_bytes(buf))
    }

    fn deserialize_i16<V>(self, v: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let mut buf = [0; 2];
        self.reader.read_exact(&mut buf)?;
        v.visit_i16(i16::from_be_bytes(buf))
    }

    fn deserialize_i32<V>(self, v: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let mut buf = [0; 4];
        self.reader.read_exact(&mut buf)?;
        v.visit_i32(i32::from_be_bytes(buf))
    }

    fn deserialize_i64<V>(self, v: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let mut buf = [0; 8];
        self.reader.read_exact(&mut buf)?;
        v.visit_i64(i64::from_be_bytes(buf))
    }

    fn deserialize_u8<V>(self, v: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let mut buf = [0; 1];
        self.reader.read_exact(&mut buf)?;
        v.visit_u8(u8::from_be_bytes(buf))
    }

    fn deserialize_u16<V>(self, v: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let mut buf = [0; 2];
        self.reader.read_exact(&mut buf.as_mut_slice())?;
        v.visit_u16(u16::from_be_bytes(buf))
    }

    fn deserialize_u32<V>(self, v: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let mut buf = [0; 4];
        self.reader.read_exact(&mut buf.as_mut_slice())?;
        v.visit_u32(u32::from_be_bytes(buf))
    }

    fn deserialize_u64<V>(self, v: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let mut buf = [0; 8];
        self.reader.read_exact(&mut buf)?;
        v.visit_u64(u64::from_be_bytes(buf))
    }

    fn deserialize_f32<V>(self, v: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let mut buf = [0; 4];
        self.reader.read_exact(&mut buf)?;
        v.visit_f32(f32::from_be_bytes(buf))
    }

    fn deserialize_f64<V>(self, v: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let mut buf = [0; 8];
        self.reader.read_exact(&mut buf)?;
        v.visit_f64(f64::from_be_bytes(buf))
    }

    fn deserialize_char<V>(self, _v: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        unimplemented!()
    }

    fn deserialize_str<V>(self, _v: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        unimplemented!()
    }

    fn deserialize_string<V>(self, _v: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        unimplemented!()
    }

    fn deserialize_bytes<V>(self, _v: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        unimplemented!()
    }

    fn deserialize_byte_buf<V>(self, _v: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        unimplemented!()
    }

    fn deserialize_option<V>(self, _v: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        unimplemented!()
    }

    fn deserialize_unit<V>(self, _v: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        unimplemented!()
    }

    fn deserialize_unit_struct<V>(self, _name: &str, _v: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        unimplemented!()
    }

    fn deserialize_newtype_struct<V>(self, _name: &str, _v: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        unimplemented!()
    }

    fn deserialize_seq<V>(self, _v: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        unimplemented!()
    }

    fn deserialize_tuple<V>(self, len: usize, v: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        v.visit_seq(SeqAccess { de: self, len })
    }

    fn deserialize_tuple_struct<V>(
        self,
        _name: &str,
        _len: usize,
        _v: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        unimplemented!()
    }

    fn deserialize_map<V>(self, _v: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        unimplemented!()
    }

    fn deserialize_struct<V>(
        self,
        _name: &str,
        fields: &[&str],
        v: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_tuple(fields.len(), v)
    }

    fn deserialize_enum<V>(
        self,
        _name: &str,
        _variants: &[&str],
        _v: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        unimplemented!()
    }

    fn deserialize_identifier<V>(self, _v: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        unimplemented!()
    }

    fn deserialize_ignored_any<V>(self, _v: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        unimplemented!()
    }
}

/// SeqAccess for Visitor
struct SeqAccess<'a, R: 'a> {
    de: &'a mut De<R>,
    len: usize,
}

impl<'a, 'de, R: Read> de::SeqAccess<'de> for SeqAccess<'a, R> {
    type Error = Error;

    fn next_element_seed<T>(&mut self, _seed: T) -> Result<Option<T::Value>, Self::Error>
    where
        T: de::DeserializeSeed<'de>,
    {
        unimplemented!()
    }

    fn next_element<T>(&mut self) -> Result<Option<T>, Self::Error>
    where
        T: Deserialize<'de>,
    {
        if self.len > 0 {
            self.len -= 1;
            T::deserialize(&mut *self.de).map(Some)
        } else {
            Ok(None)
        }
    }
}

/// Deserialize default value (zero) as None
pub fn custom_option<'de, D, T>(d: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de> + Default + Copy + PartialEq + Eq,
{
    match T::deserialize(d) {
        Ok(value) if value == T::default() => Ok(None),
        Ok(value) => Ok(Some(value)),
        Err(e) => Err(e),
    }
}

pub fn from_bytes<'de, T>(input: &'de [u8]) -> Result<T, Error>
where
    T: Deserialize<'de> + Block,
{
    from_reader(input)
}

pub fn from_reader<'de, T, R>(mut reader: R) -> Result<T, Error>
where
    T: Deserialize<'de> + Block,
    R: Read,
{
    let mut buf = Vec::<u8>::new();
    buf.try_reserve(T::block_size())
        .map_err(Error::OutOfMemory)?;
    buf.resize(T::block_size(), 0);
    reader.read_exact(&mut buf).map_err(Error::IoError)?;
    T::deserialize(&mut De::from_reader(std::io::Cursor::new(buf)))
}
