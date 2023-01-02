///! Sqlite data format serializer
use crate::error::Error;
use block::Block;
use serde::{
    ser::SerializeMap, ser::SerializeSeq, ser::SerializeStruct, ser::SerializeStructVariant,
    ser::SerializeTuple, ser::SerializeTupleStruct, ser::SerializeTupleVariant, Serialize,
    Serializer,
};
use std::io::{BufWriter, Write};

struct SqliteSe<W: Write> {
    writer: W,
}

impl<'a, W: Write> Serializer for &'a mut SqliteSe<W> {
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

    fn serialize_str(self, _: &str) -> Result<Self::Ok, Self::Error> {
        Err(Error::Unsupported("Serializer::serialize_str"))
    }

    fn serialize_bytes(self, _: &[u8]) -> Result<Self::Ok, Self::Error> {
        Err(Error::Unsupported("Serializer::serialize_bytes"))
    }

    fn serialize_none(self) -> Result<Self::Ok, Self::Error> {
        Err(Error::Unsupported("Serializer::serialize_none"))
    }

    fn serialize_some<T: ?Sized + Serialize>(self, _: &T) -> Result<Self::Ok, Self::Error> {
        Err(Error::Unsupported("Serializer::serialize_some"))
    }

    fn serialize_unit(self) -> Result<Self::Ok, Self::Error> {
        Err(Error::Unsupported("Serializer::serialize_unit"))
    }

    fn serialize_unit_struct(self, _name: &str) -> Result<Self::Ok, Self::Error> {
        Err(Error::Unsupported("Serializer::serialize_unit_struct"))
    }

    fn serialize_unit_variant(
        self,
        _name: &str,
        _variant_index: u32,
        _variant: &str,
    ) -> Result<Self::Ok, Self::Error> {
        Err(Error::Unsupported("Serializer::serialize_unit_variant"))
    }

    fn serialize_newtype_struct<T: ?Sized + Serialize>(
        self,
        _name: &str,
        _value: &T,
    ) -> Result<Self::Ok, Self::Error> {
        Err(Error::Unsupported("Serializer::serialize_newtype_struct"))
    }

    fn serialize_newtype_variant<T: ?Sized + Serialize>(
        self,
        _name: &str,
        variant_index: u32,
        _variant: &str,
        value: &T,
    ) -> Result<Self::Ok, Self::Error> {
        self.writer.write_all(&variant_index.to_be_bytes())?;
        value.serialize(self)
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

impl<'a, W: Write> SerializeSeq for &'a mut SqliteSe<W> {
    type Ok = ();
    type Error = Error;

    fn serialize_element<T>(&mut self, _value: &T) -> Result<Self::Ok, Self::Error>
    where
        T: ?Sized + Serialize,
    {
        Err(Error::Unsupported("SerializeSeq::serialize_element"))
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        Err(Error::Unsupported("SerializeSeq::end"))
    }
}

impl<'a, W: Write> SerializeTuple for &'a mut SqliteSe<W> {
    type Ok = ();
    type Error = Error;

    fn serialize_element<T>(&mut self, value: &T) -> Result<Self::Ok, Self::Error>
    where
        T: ?Sized + Serialize,
    {
        value.serialize(&mut **self)
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        Ok(())
    }
}

impl<'a, W: Write> SerializeTupleStruct for &'a mut SqliteSe<W> {
    type Ok = ();
    type Error = Error;

    fn serialize_field<T>(&mut self, _value: &T) -> Result<Self::Ok, Self::Error>
    where
        T: ?Sized + Serialize,
    {
        Err(Error::Unsupported("SerializeTupleStruct::serialize_field"))
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        Err(Error::Unsupported("SerializeTupleStruct::end"))
    }
}

impl<'a, W: Write> SerializeTupleVariant for &'a mut SqliteSe<W> {
    type Ok = ();
    type Error = Error;

    fn serialize_field<T>(&mut self, _value: &T) -> Result<Self::Ok, Self::Error>
    where
        T: ?Sized + Serialize,
    {
        Err(Error::Unsupported("SerializeTupleVariant::serialize_field"))
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        Err(Error::Unsupported("SerializeTupleVariant::end"))
    }
}

impl<'a, W: Write> SerializeMap for &'a mut SqliteSe<W> {
    type Ok = ();
    type Error = Error;

    fn serialize_key<T>(&mut self, _key: &T) -> Result<Self::Ok, Self::Error>
    where
        T: ?Sized + Serialize,
    {
        Err(Error::Unsupported("SerializeMap::serialize_key"))
    }

    fn serialize_value<T>(&mut self, _value: &T) -> Result<Self::Ok, Self::Error>
    where
        T: ?Sized + Serialize,
    {
        Err(Error::Unsupported("SerializeMap::serialize_value"))
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        Err(Error::Unsupported("SerializeMap::end"))
    }
}

impl<'a, W: Write> SerializeStruct for &'a mut SqliteSe<W> {
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

impl<'a, W: Write> SerializeStructVariant for &'a mut SqliteSe<W> {
    type Ok = ();
    type Error = Error;

    fn serialize_field<T>(&mut self, _key: &str, _value: &T) -> Result<Self::Ok, Self::Error>
    where
        T: ?Sized + Serialize,
    {
        Err(Error::Unsupported(
            "SerializeStructVariant::serialize_field",
        ))
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        Err(Error::Unsupported("SerializeStructVariant::end"))
    }
}

// serialize None as zero
pub fn none_as_zero<S, T>(field: &Option<T>, s: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
    T: Serialize + Copy + Default,
{
    let value = match field.as_ref() {
        None => T::default(),
        Some(&v) => v,
    };
    value.serialize(s)
}

struct CountingBufWriter<W: Write> {
    writer: BufWriter<W>,
    written: usize,
    block_size: usize,
}

impl<W: Write> CountingBufWriter<W> {
    fn new(writer: W, block_size: usize) -> Self {
        Self {
            writer: BufWriter::new(writer),
            written: 0,
            block_size,
        }
    }

    fn pad(&mut self) -> std::io::Result<()> {
        let mut left = self.block_size - self.written;
        if left == 0 {
            return Ok(());
        }
        let buf_size = 4096;
        let mut buf = vec![0; 4096];
        while left > 0 {
            let to_write = buf_size.min(left);
            // *safe* since vec is pre-allocated and initialized
            unsafe { buf.set_len(to_write) };
            self.write_all(buf.as_mut_slice())?;
            left -= to_write
        }
        Ok(())
    }
}

impl<W: Write> Write for CountingBufWriter<W> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if self.written + buf.len() > self.block_size {
            // FIXME:
            return Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "block size overflow",
            ));
        }
        let written = self.writer.write(buf)?;
        self.written += written;
        Ok(written)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.writer.flush()
    }
}

pub fn to_bytes<T>(value: &T) -> Result<Vec<u8>, Error>
where
    T: Serialize + Block,
{
    let mut buf = Vec::<u8>::new();
    buf.try_reserve(value.iblock_size())
        .map_err(Error::OutOfMemory)?;
    buf.resize(value.iblock_size(), 0);
    to_writer(buf.as_mut_slice(), value)?;
    Ok(buf)
}

pub fn to_writer<T, W: Write>(writer: W, value: &T) -> Result<(), Error>
where
    T: Serialize + Block,
{
    let mut cbw = CountingBufWriter::new(writer, value.iblock_size());
    value.serialize(&mut SqliteSe { writer: &mut cbw })?;
    cbw.pad()?;
    Ok(cbw.flush()?)
}
