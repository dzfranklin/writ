use ring::digest::{Algorithm, Context, Digest};
use std::{fmt, io};

pub struct WithDigest<W> {
    inner: W,
    ctx: Context,
}

impl<W> WithDigest<W> {
    pub fn new(algorithm: &'static Algorithm, inner: W) -> Self {
        Self {
            inner,
            ctx: Context::new(algorithm),
        }
    }

    pub fn finish(self) -> Digest {
        self.ctx.finish()
    }
}

impl<W: io::Write> io::Write for WithDigest<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let written = self.inner.write(buf)?;
        self.ctx.update(&buf[0..written]);
        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

impl<R: io::Read> io::Read for WithDigest<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let read = self.inner.read(buf)?;
        self.ctx.update(&buf[0..read]);
        Ok(read)
    }
}

impl<W: fmt::Debug> fmt::Debug for WithDigest<W> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WithDigest")
            .field("inner", &self.inner)
            .finish()
    }
}
