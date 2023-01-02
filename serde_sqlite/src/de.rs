//! SQLite data format deserializer

use crate::error::Error;
use block::Block;
use serde::{
    de, de::DeserializeSeed, de::IntoDeserializer, de::Visitor, Deserialize, Deserializer,
};
use std::io::Read;

struct SqliteDe<R> {
    reader: R,
}

impl<R: Read> SqliteDe<R> {
    fn from_reader(reader: R) -> Self {
        Self { reader }
    }
}

impl<'de, 'a, R> Deserializer<'de> for &'a mut SqliteDe<R>
where
    R: Read,
{
    type Error = Error;

    fn deserialize_any<V>(self, _v: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        Err(Self::Error::Unsupported("Deserializer::deserialize_any"))
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
        self.reader.read_exact(buf.as_mut_slice())?;
        v.visit_i8(i8::from_be_bytes(buf))
    }

    fn deserialize_i16<V>(self, v: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let mut buf = [0; 2];
        self.reader.read_exact(buf.as_mut_slice())?;
        v.visit_i16(i16::from_be_bytes(buf))
    }

    fn deserialize_i32<V>(self, v: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let mut buf = [0; 4];
        self.reader.read_exact(buf.as_mut_slice())?;
        v.visit_i32(i32::from_be_bytes(buf))
    }

    fn deserialize_i64<V>(self, v: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let mut buf = [0; 8];
        self.reader.read_exact(buf.as_mut_slice())?;
        v.visit_i64(i64::from_be_bytes(buf))
    }

    fn deserialize_u8<V>(self, v: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let mut buf = [0; 1];
        self.reader.read_exact(buf.as_mut_slice())?;
        v.visit_u8(u8::from_be_bytes(buf))
    }

    fn deserialize_u16<V>(self, v: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let mut buf = [0; 2];
        self.reader.read_exact(buf.as_mut_slice())?;
        v.visit_u16(u16::from_be_bytes(buf))
    }

    fn deserialize_u32<V>(self, v: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let mut buf = [0; 4];
        self.reader.read_exact(buf.as_mut_slice())?;
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
        Err(Self::Error::Unsupported("Deserializer::deserialize_char"))
    }

    fn deserialize_str<V>(self, _v: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        Err(Error::Unsupported("Deserializer::deserialize_str"))
    }

    fn deserialize_string<V>(self, _v: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        Err(Error::Unsupported("Deserializer::deserialize_string"))
    }

    fn deserialize_bytes<V>(self, _v: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        Err(Error::Unsupported("Deserializer::deserialize_bytes"))
    }

    fn deserialize_byte_buf<V>(self, _v: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        Err(Error::Unsupported("Deserializer::deserialize_byte_buf"))
    }

    fn deserialize_option<V>(self, _v: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        Err(Error::Unsupported("Deserializer::deserialize_option"))
    }

    fn deserialize_unit<V>(self, _v: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        Err(Error::Unsupported("Deserializer::deserialize_unit"))
    }

    fn deserialize_unit_struct<V>(self, _name: &str, _v: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        Err(Error::Unsupported("Deserializer::deserialize_unit_struct"))
    }

    fn deserialize_newtype_struct<V>(self, _name: &str, _v: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        Err(Error::Unsupported(
            "Deserializer::deserialize_newtype_struct",
        ))
    }

    fn deserialize_seq<V>(self, _v: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        Err(Error::Unsupported("Deserializer::deserialize_seq"))
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
        Err(Error::Unsupported("Deserializer::deserialize_tuple_struct"))
    }

    fn deserialize_map<V>(self, _v: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        Err(Error::Unsupported("Deserializer::deserialize_map"))
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
        v: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        v.visit_enum(EnumAccess::new(self))
    }

    fn deserialize_identifier<V>(self, _v: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        Err(Error::Unsupported("Deserializer::deserialize_identifier"))
    }

    fn deserialize_ignored_any<V>(self, _v: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        Err(Error::Unsupported("Deserializer::deserialize_ignored_any"))
    }
}

struct EnumAccess<'a, R: 'a> {
    de: &'a mut SqliteDe<R>,
}

impl<'a, R> EnumAccess<'a, R> {
    fn new(de: &'a mut SqliteDe<R>) -> Self {
        Self { de }
    }
}

impl<'a, 'de, R: Read> de::EnumAccess<'de> for EnumAccess<'a, R> {
    type Error = Error;
    type Variant = VariantAccess<'a, R>;

    fn variant<V>(self) -> Result<(V, Self::Variant), Self::Error>
    where
        V: Deserialize<'de>,
    {
        let mut buf = [0_u8; 4];
        self.de.reader.read_exact(&mut buf)?;
        let tag = u32::from_be_bytes(buf) as u64;
        let de = IntoDeserializer::<Error>::into_deserializer(tag);
        let tag = V::deserialize(de)?;
        Ok((tag, VariantAccess { de: self.de }))
    }

    fn variant_seed<V>(self, _seed: V) -> Result<(V::Value, Self::Variant), Error>
    where
        V: DeserializeSeed<'de>,
    {
        Err(Error::Unsupported("EnumAccess::variant_seed"))
    }
}

struct VariantAccess<'a, R: 'a> {
    de: &'a mut SqliteDe<R>,
}

impl<'a, 'de, R: Read> de::VariantAccess<'de> for VariantAccess<'a, R> {
    type Error = Error;

    fn unit_variant(self) -> Result<(), Self::Error> {
        Err(Error::Unsupported("VariantAccess::unit_variant"))
    }

    fn newtype_variant_seed<T>(self, _seed: T) -> Result<T::Value, Self::Error>
    where
        T: DeserializeSeed<'de>,
    {
        Err(Error::Unsupported("VariantAccess::newtype_variant_seed"))
    }

    fn tuple_variant<V>(self, _len: usize, _visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        Err(Error::Unsupported("VariantAccess::tuple_variant"))
    }

    fn struct_variant<V>(
        self,
        _fields: &'static [&'static str],
        _visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        Err(Error::Unsupported("VariantAccess::struct_variant"))
    }

    fn newtype_variant<T>(self) -> Result<T, Self::Error>
    where
        T: Deserialize<'de>,
    {
        T::deserialize(self.de)
    }
}

/// SeqAccess Visitor
struct SeqAccess<'a, R: 'a> {
    de: &'a mut SqliteDe<R>,
    len: usize,
}

impl<'a, 'de, R: Read> de::SeqAccess<'de> for SeqAccess<'a, R> {
    type Error = Error;

    fn next_element_seed<T>(&mut self, _seed: T) -> Result<Option<T::Value>, Self::Error>
    where
        T: de::DeserializeSeed<'de>,
    {
        Err(Error::Unsupported("SeqAccess::next_element"))
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

struct CountingReader<R: Read> {
    reader: R,
    read: usize,
}

impl<R: std::io::Read> CountingReader<R> {
    fn new(reader: R) -> Self {
        Self { reader, read: 0 }
    }

    fn discard_padding(&mut self, left: usize) -> std::io::Result<()> {
        if left == 0 {
            return Ok(());
        }
        let mut buf = vec![0; left];
        self.read_exact(buf.as_mut_slice())?;
        Ok(())
    }
}

impl<R: Read> Read for CountingReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let read = self.reader.read(buf)?;
        self.read += read;
        Ok(read)
    }
}

/// Deserialize default value (zero) as None
pub fn zero_as_none<'de, D, T>(d: D) -> Result<Option<T>, D::Error>
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

pub fn from_reader<'de, T, R>(reader: R) -> Result<T, Error>
where
    T: Deserialize<'de> + Block,
    R: Read,
{
    let mut cbr = CountingReader::new(reader);
    let res = T::deserialize(&mut SqliteDe::from_reader(&mut cbr))?;
    cbr.discard_padding(res.iblock_size() - cbr.read)?;
    Ok(res)
}
