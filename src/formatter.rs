// Custom serde_json formatter supporting ReJSON formatting options.

use serde_json::ser::Formatter;
use std::io;

pub struct RedisJsonFormatter<'a> {
    current_indent: usize,
    has_value: bool,
    indent: &'a [u8],
    space: &'a [u8],
    newline: &'a [u8],
}

impl<'a> RedisJsonFormatter<'a> {
    pub fn new(indent: &'a [u8], space: &'a [u8], newline: &'a [u8]) -> Self {
        RedisJsonFormatter {
            current_indent: 0,
            has_value: false,
            indent,
            space,
            newline,
        }
    }

    fn indent<W: ?Sized>(wr: &mut W, n: usize, s: &[u8]) -> io::Result<()>
    where
        W: io::Write,
    {
        for _ in 0..n {
            wr.write_all(s)?;
        }

        Ok(())
    }
}

impl<'a> Formatter for RedisJsonFormatter<'a> {
    fn begin_array<W: ?Sized>(&mut self, writer: &mut W) -> io::Result<()>
    where
        W: io::Write,
    {
        self.current_indent += 1;
        self.has_value = false;
        writer.write_all(b"[")
    }

    fn end_array<W: ?Sized>(&mut self, writer: &mut W) -> io::Result<()>
    where
        W: io::Write,
    {
        self.current_indent -= 1;

        if self.has_value {
            writer.write_all(self.newline)?;
            Self::indent(writer, self.current_indent, self.indent)?;
        }

        writer.write_all(b"]")
    }

    fn begin_array_value<W: ?Sized>(&mut self, writer: &mut W, first: bool) -> io::Result<()>
    where
        W: io::Write,
    {
        if !first {
            writer.write_all(b",")?;
        }
        writer.write_all(self.newline)?;
        Self::indent(writer, self.current_indent, self.indent)
    }

    fn end_array_value<W: ?Sized>(&mut self, _writer: &mut W) -> io::Result<()>
    where
        W: io::Write,
    {
        self.has_value = true;
        Ok(())
    }

    fn begin_object<W: ?Sized>(&mut self, writer: &mut W) -> io::Result<()>
    where
        W: io::Write,
    {
        self.current_indent += 1;
        self.has_value = false;
        writer.write_all(b"{")
    }

    fn end_object<W: ?Sized>(&mut self, writer: &mut W) -> io::Result<()>
    where
        W: io::Write,
    {
        self.current_indent -= 1;

        if self.has_value {
            writer.write_all(self.newline)?;
            Self::indent(writer, self.current_indent, self.indent)?;
        }

        writer.write_all(b"}")
    }

    fn begin_object_key<W: ?Sized>(&mut self, writer: &mut W, first: bool) -> io::Result<()>
    where
        W: io::Write,
    {
        if !first {
            writer.write_all(b",")?;
        }
        writer.write_all(self.newline)?;
        Self::indent(writer, self.current_indent, self.indent)
    }

    fn begin_object_value<W: ?Sized>(&mut self, writer: &mut W) -> io::Result<()>
    where
        W: io::Write,
    {
        writer.write_all(b":")?;
        writer.write_all(self.space)
    }

    fn end_object_value<W: ?Sized>(&mut self, _writer: &mut W) -> io::Result<()>
    where
        W: io::Write,
    {
        self.has_value = true;
        Ok(())
    }
}
