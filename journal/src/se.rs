///! Journal Data Format Serializer
use crate::error::Error;
use block::Block;
use serde::{ser, Serialize};
use std::io::Write;

struct Se<W: Write> {
    writer: W,
}

impl<'a, W: Write> ser::Serializer for &'a mut Se<W> {
    type Ok = ();
    type Error = Error;

    type SerializeSeq = Self;
    type SerializeTuple = Self;
    type SerializeTupleStruct = Self;
    type SerializeTupleVariant = Self;
    type SerializeMap = Self;
    type SerializeStruct = Self;
    type SerializeStructVariant = Self;

    fn serialize_bool(self, value: bool) -> Result<Self::Ok, Self::Error> {
        self.writer
            .write_all(&(value as u8).to_be_bytes())
            .map_err(Into::into)
    }

    fn serialize_i8(self, value: i8) -> Result<Self::Ok, Self::Error> {
        self.writer
            .write_all(&value.to_be_bytes())
            .map_err(Into::into)
    }

    fn serialize_i16(self, value: i16) -> Result<Self::Ok, Self::Error> {
        self.writer
            .write_all(&value.to_be_bytes())
            .map_err(Into::into)
    }

    fn serialize_i32(self, value: i32) -> Result<Self::Ok, Self::Error> {
        self.writer
            .write_all(&value.to_be_bytes())
            .map_err(Into::into)
    }

    fn serialize_i64(self, value: i64) -> Result<Self::Ok, Self::Error> {
        self.writer
            .write_all(&value.to_be_bytes())
            .map_err(Into::into)
    }

    fn serialize_u8(self, value: u8) -> Result<Self::Ok, Self::Error> {
        self.writer
            .write_all(&value.to_be_bytes())
            .map_err(Into::into)
    }

    fn serialize_u16(self, value: u16) -> Result<Self::Ok, Self::Error> {
        self.writer
            .write_all(&value.to_be_bytes())
            .map_err(Into::into)
    }

    fn serialize_u32(self, value: u32) -> Result<Self::Ok, Self::Error> {
        self.writer
            .write_all(&value.to_be_bytes())
            .map_err(Into::into)
    }

    fn serialize_u64(self, value: u64) -> Result<Self::Ok, Self::Error> {
        self.writer
            .write_all(&value.to_be_bytes())
            .map_err(Into::into)
    }

    fn serialize_f32(self, value: f32) -> Result<Self::Ok, Self::Error> {
        self.writer
            .write_all(&value.to_be_bytes())
            .map_err(Into::into)
    }

    fn serialize_f64(self, value: f64) -> Result<Self::Ok, Self::Error> {
        self.writer
            .write_all(&value.to_be_bytes())
            .map_err(Into::into)
    }

    fn serialize_char(self, value: char) -> Result<Self::Ok, Self::Error> {
        // char is always 4 bytes long
        self.writer
            .write_all(&(value as u32).to_be_bytes())
            .map_err(Into::into)
    }

    // FIXME: not yet supported, and probably, will never be needed
    fn serialize_str(self, _: &str) -> Result<Self::Ok, Self::Error> {
        unimplemented!()
    }

    // FIXME: not yet supported, and probably, will never be needed
    fn serialize_bytes(self, _: &[u8]) -> Result<Self::Ok, Self::Error> {
        unimplemented!()
    }

    fn serialize_none(self) -> Result<Self::Ok, Self::Error> {
        unimplemented!()
    }

    fn serialize_some<T: ?Sized + Serialize>(self, _: &T) -> Result<Self::Ok, Self::Error> {
        unimplemented!()
    }

    fn serialize_unit(self) -> Result<Self::Ok, Self::Error> {
        unimplemented!()
    }

    fn serialize_unit_struct(self, _name: &str) -> Result<Self::Ok, Self::Error> {
        unimplemented!()
    }

    fn serialize_unit_variant(
        self,
        _name: &str,
        _variant_index: u32,
        _variant: &str,
    ) -> Result<Self::Ok, Self::Error> {
        unimplemented!()
    }

    fn serialize_newtype_struct<T: ?Sized + Serialize>(
        self,
        _name: &str,
        _value: &T,
    ) -> Result<Self::Ok, Self::Error> {
        unimplemented!()
    }

    fn serialize_newtype_variant<T: ?Sized + Serialize>(
        self,
        _name: &str,
        _variant_index: u32,
        _variant: &str,
        _value: &T,
    ) -> Result<Self::Ok, Self::Error> {
        unimplemented!()
    }

    fn serialize_seq(self, _len: Option<usize>) -> Result<Self::SerializeSeq, Self::Error> {
        Ok(self)
    }

    fn serialize_tuple(self, _len: usize) -> Result<Self::SerializeTuple, Self::Error> {
        Ok(self)
    }

    fn serialize_tuple_struct(
        self,
        _name: &str,
        _len: usize,
    ) -> Result<Self::SerializeTupleStruct, Self::Error> {
        Ok(self)
    }

    fn serialize_tuple_variant(
        self,
        _name: &str,
        _variant_index: u32,
        _variant: &str,
        _len: usize,
    ) -> Result<Self::SerializeTupleVariant, Self::Error> {
        Ok(self)
    }

    fn serialize_map(self, _len: Option<usize>) -> Result<Self::SerializeMap, Self::Error> {
        Ok(self)
    }

    fn serialize_struct(
        self,
        _name: &str,
        _len: usize,
    ) -> Result<Self::SerializeStruct, Self::Error> {
        Ok(self)
    }

    fn serialize_struct_variant(
        self,
        _name: &str,
        _variant_index: u32,
        _variant: &str,
        _len: usize,
    ) -> Result<Self::SerializeTupleVariant, Self::Error> {
        Ok(self)
    }
}

impl<'a, W: Write> ser::SerializeSeq for &'a mut Se<W> {
    type Ok = ();
    type Error = Error;

    fn serialize_element<T>(&mut self, _value: &T) -> Result<Self::Ok, Self::Error>
    where
        T: ?Sized + Serialize,
    {
        unimplemented!()
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        unimplemented!()
    }
}

impl<'a, W: Write> ser::SerializeTuple for &'a mut Se<W> {
    type Ok = ();
    type Error = Error;

    fn serialize_element<T>(&mut self, _value: &T) -> Result<Self::Ok, Self::Error>
    where
        T: ?Sized + Serialize,
    {
        unimplemented!()
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        unimplemented!()
    }
}

impl<'a, W: Write> ser::SerializeTupleStruct for &'a mut Se<W> {
    type Ok = ();
    type Error = Error;

    fn serialize_field<T>(&mut self, _value: &T) -> Result<Self::Ok, Self::Error>
    where
        T: ?Sized + Serialize,
    {
        unimplemented!()
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        unimplemented!()
    }
}

impl<'a, W: Write> ser::SerializeTupleVariant for &'a mut Se<W> {
    type Ok = ();
    type Error = Error;

    fn serialize_field<T>(&mut self, _value: &T) -> Result<Self::Ok, Self::Error>
    where
        T: ?Sized + Serialize,
    {
        unimplemented!()
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        unimplemented!()
    }
}

impl<'a, W: Write> ser::SerializeMap for &'a mut Se<W> {
    type Ok = ();
    type Error = Error;

    fn serialize_key<T>(&mut self, _key: &T) -> Result<Self::Ok, Self::Error>
    where
        T: ?Sized + Serialize,
    {
        unimplemented!()
    }

    fn serialize_value<T>(&mut self, _value: &T) -> Result<Self::Ok, Self::Error>
    where
        T: ?Sized + Serialize,
    {
        unimplemented!()
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        Ok(())
    }
}

impl<'a, W: Write> ser::SerializeStruct for &'a mut Se<W> {
    type Ok = ();
    type Error = Error;

    fn serialize_field<T>(&mut self, _key: &str, value: &T) -> Result<Self::Ok, Self::Error>
    where
        T: ?Sized + Serialize,
    {
        value.serialize(&mut **self)
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        Ok(())
    }
}

impl<'a, W: Write> ser::SerializeStructVariant for &'a mut Se<W> {
    type Ok = ();
    type Error = Error;

    fn serialize_field<T>(&mut self, _key: &str, _value: &T) -> Result<Self::Ok, Self::Error>
    where
        T: ?Sized + Serialize,
    {
        unimplemented!()
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        unimplemented!()
    }
}

/// serialize None as zero
pub fn custom_option<S, T>(field: &Option<T>, s: S) -> Result<S::Ok, S::Error>
where
    S: ser::Serializer,
    T: Serialize + Copy + Default,
{
    let value = match field.as_ref() {
        None => T::default(),
        Some(&v) => v,
    };
    value.serialize(s)
}

pub fn to_bytes<T>(value: &T) -> Result<Vec<u8>, Error>
where
    T: Serialize + Block,
{
    let mut buf = Vec::<u8>::new();
    buf.try_reserve(T::block_size())
        .map_err(Error::OutOfMemory)?;
    buf.resize(T::block_size(), 0);
    to_writer(buf.as_mut_slice(), value)?;
    Ok(buf)
}

// FIXME:
// this func doesn't check block if block was fully written
fn to_writer<T, W: Write>(writer: W, value: &T) -> Result<(), Error>
where
    T: Serialize,
{
    value.serialize(&mut Se { writer })
}
