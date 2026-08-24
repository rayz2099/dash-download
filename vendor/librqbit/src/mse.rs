//! MSE/PE 出站握手. 国内迅雷/BitComet 默认加密, 明文握手完不发 bitfield.
//! 规范: wiki.vuze.com/w/Message_Stream_Encryption

use std::io::IoSlice;
use std::pin::Pin;
use std::task::{Context, Poll};

use anyhow::{Context as _, bail};
use num_bigint::BigUint;
use num_traits::Num;
use rand::Rng;
use sha1w::{ISha1, Sha1};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, ReadBuf};

use crate::type_aliases::{BoxAsyncReadVectored, BoxAsyncWrite};
use crate::vectored_traits::AsyncReadVectoredIntoCompat;

const P_HEX: &str = "FFFFFFFFFFFFFFFFC90FDAA22168C234C4C6628B80DC1CD129024E088A67CC74020BBEA63B139B22514A08798E3404DDEF9519B3CD3A431B302B0A6DF25F14374FE1356D6D51C245E485B576625E7EC6F44C42E9A63A36210000000000090563";
const CRYPTO_RC4: u32 = 2;
const CRYPTO_PLAIN: u32 = 1;
const PSTR: &[u8] = b"BitTorrent protocol";

fn sha1_parts(parts: &[&[u8]]) -> [u8; 20] {
    let mut h = Sha1::new();
    for p in parts {
        h.update(p);
    }
    h.finish()
}

struct Rc4 {
    s: [u8; 256],
    i: u8,
    j: u8,
}

impl Rc4 {
    fn new(key: &[u8]) -> Self {
        let mut s = [0u8; 256];
        for (i, v) in s.iter_mut().enumerate() {
            *v = i as u8;
        }
        let mut j: u8 = 0;
        for i in 0..256 {
            j = j.wrapping_add(s[i]).wrapping_add(key[i % key.len()]);
            s.swap(i, j as usize);
        }
        let mut rc4 = Self { s, i: 0, j: 0 };
        let mut burn = [0u8; 1024];
        rc4.xor(&mut burn);
        rc4
    }

    fn xor(&mut self, data: &mut [u8]) {
        for b in data {
            self.i = self.i.wrapping_add(1);
            self.j = self.j.wrapping_add(self.s[self.i as usize]);
            self.s.swap(self.i as usize, self.j as usize);
            let k = self.s[self.i as usize].wrapping_add(self.s[self.j as usize]);
            *b ^= self.s[k as usize];
        }
    }

    fn crypt(&mut self, data: &[u8]) -> Vec<u8> {
        let mut o = data.to_vec();
        self.xor(&mut o);
        o
    }
}

fn to_96(n: &BigUint) -> [u8; 96] {
    let b = n.to_bytes_be();
    let mut out = [0u8; 96];
    let n = b.len().min(96);
    out[96 - n..].copy_from_slice(&b[b.len() - n..]);
    out
}

fn p() -> BigUint {
    BigUint::from_str_radix(P_HEX, 16).unwrap()
}

fn dh_public(xa: &BigUint) -> [u8; 96] {
    to_96(&BigUint::from(2u32).modpow(xa, &p()))
}

fn dh_secret(ya: &[u8], xa: &BigUint) -> [u8; 96] {
    to_96(&BigUint::from_bytes_be(ya).modpow(xa, &p()))
}

fn new_xa() -> BigUint {
    let mut xb = [0u8; 20];
    rand::rng().fill_bytes(&mut xb);
    BigUint::from_bytes_be(&xb)
}

fn prepend(prefix: Vec<u8>, inner: BoxAsyncReadVectored) -> BoxAsyncReadVectored {
    if prefix.is_empty() {
        inner
    } else {
        Box::new(
            PrefixRead {
                prefix,
                prefix_pos: 0,
                inner,
            }
            .into_vectored_compat(),
        )
    }
}

fn wrap_pair(
    read: BoxAsyncReadVectored,
    write: BoxAsyncWrite,
    leftover: Vec<u8>,
    read_rc4: Option<Rc4>,
    write_rc4: Option<Rc4>,
) -> (BoxAsyncReadVectored, BoxAsyncWrite) {
    let read: BoxAsyncReadVectored = match read_rc4 {
        Some(rc4) => Box::new(Rc4Read { inner: read, rc4 }.into_vectored_compat()),
        None => read,
    };
    let write: BoxAsyncWrite = match write_rc4 {
        Some(rc4) => Box::new(Rc4Write {
            inner: write,
            rc4,
            pending: Vec::new(),
            pos: 0,
        }),
        None => write,
    };
    (prepend(leftover, read), write)
}

struct Rc4Read<R> {
    inner: R,
    rc4: Rc4,
}

impl<R: AsyncRead + Unpin> AsyncRead for Rc4Read<R> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let filled0 = buf.filled().len();
        let this = self.as_mut().get_mut();
        match Pin::new(&mut this.inner).poll_read(cx, buf) {
            Poll::Ready(Ok(())) => {
                let filled = buf.filled_mut();
                this.rc4.xor(&mut filled[filled0..]);
                Poll::Ready(Ok(()))
            }
            other => other,
        }
    }
}

struct Rc4Write<W> {
    inner: W,
    rc4: Rc4,
    pending: Vec<u8>,
    pos: usize,
}

impl<W: AsyncWrite + Unpin> AsyncWrite for Rc4Write<W> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        let this = self.as_mut().get_mut();
        if this.pending.is_empty() {
            this.pending = buf.to_vec();
            this.rc4.xor(&mut this.pending);
            this.pos = 0;
        }
        while this.pos < this.pending.len() {
            match Pin::new(&mut this.inner).poll_write(cx, &this.pending[this.pos..]) {
                Poll::Ready(Ok(0)) => {
                    return Poll::Ready(Err(std::io::Error::new(
                        std::io::ErrorKind::WriteZero,
                        "mse write zero",
                    )));
                }
                Poll::Ready(Ok(n)) => this.pos += n,
                Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
                Poll::Pending => return Poll::Pending,
            }
        }
        let n = buf.len();
        this.pending.clear();
        this.pos = 0;
        Poll::Ready(Ok(n))
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.as_mut().get_mut().inner).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.as_mut().get_mut().inner).poll_shutdown(cx)
    }

    fn poll_write_vectored(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bufs: &[IoSlice<'_>],
    ) -> Poll<std::io::Result<usize>> {
        let b = bufs.iter().find(|b| !b.is_empty()).map(|b| &b[..]).unwrap_or(&[]);
        self.poll_write(cx, b)
    }
}

/// 出站 MSE. 失败则调用方应保持原明文路径或放弃该 peer.
pub async fn initiate(
    mut read: BoxAsyncReadVectored,
    mut write: BoxAsyncWrite,
    skey: &[u8],
) -> anyhow::Result<(BoxAsyncReadVectored, BoxAsyncWrite)> {
    let xa = new_xa();
    let ya = dh_public(&xa);

    let mut pad_a = [0u8; 16];
    rand::rng().fill_bytes(&mut pad_a);
    let mut pkt = Vec::with_capacity(112);
    pkt.extend_from_slice(&ya);
    pkt.extend_from_slice(&pad_a);
    write.write_all(&pkt).await.context("mse write Ya")?;
    write.flush().await.context("mse flush Ya")?;

    let mut yb = [0u8; 96];
    read.read_exact(&mut yb).await.context("mse read Yb")?;
    let s = dh_secret(&yb, &xa);

    let mut enc_a = Rc4::new(&sha1_parts(&[b"keyA", &s, skey]));
    let mut enc_b_sync = Rc4::new(&sha1_parts(&[b"keyB", &s, skey]));
    let vc_enc = enc_b_sync.crypt(&[0u8; 8]);

    let mut pad_c = [0u8; 8];
    rand::rng().fill_bytes(&mut pad_c);
    let mut body = Vec::new();
    body.extend_from_slice(&[0u8; 8]);
    body.extend_from_slice(&3u32.to_be_bytes()); // plaintext | rc4
    body.extend_from_slice(&(pad_c.len() as u16).to_be_bytes());
    body.extend_from_slice(&pad_c);
    body.extend_from_slice(&0u16.to_be_bytes()); // no IA
    let enc_body = enc_a.crypt(&body);

    let mut pkt2 = Vec::new();
    pkt2.extend_from_slice(&sha1_parts(&[b"req1", &s]));
    let xored: Vec<u8> = sha1_parts(&[b"req2", skey])
        .iter()
        .zip(sha1_parts(&[b"req3", &s]).iter())
        .map(|(a, b)| a ^ b)
        .collect();
    pkt2.extend_from_slice(&xored);
    pkt2.extend_from_slice(&enc_body);
    write.write_all(&pkt2).await.context("mse write req")?;
    write.flush().await.context("mse flush req")?;

    // padB (最多 512) + 加密 VC. 在 520 字节窗口里找 VC.
    let mut window = Vec::with_capacity(520);
    let mut tmp = [0u8; 64];
    loop {
        if window.len() >= 8 {
            if let Some(pos) = window.windows(8).position(|w| w == vc_enc.as_slice()) {
                let rest = window.split_off(pos + 8);
                let mut enc_b = Rc4::new(&sha1_parts(&[b"keyB", &s, skey]));
                let _ = enc_b.crypt(&[0u8; 8]); // 与 VC 对齐
                // rest 是 VC 之后的原始字节: 先解密 method+padD, 多出来的按 method 决定是否再 XOR
                return finish_select(read, write, enc_a, enc_b, rest).await;
            }
        }
        if window.len() >= 520 {
            bail!("mse: VC not found");
        }
        let n = read.read(&mut tmp).await.context("mse read vc")?;
        if n == 0 {
            bail!("mse: eof waiting VC");
        }
        window.extend_from_slice(&tmp[..n]);
        if window.len() > 520 {
            let drop = window.len() - 520;
            window.drain(..drop);
        }
    }
}

async fn finish_select(
    mut read: BoxAsyncReadVectored,
    write: BoxAsyncWrite,
    enc_a: Rc4,
    mut enc_b: Rc4,
    mut raw: Vec<u8>,
) -> anyhow::Result<(BoxAsyncReadVectored, BoxAsyncWrite)> {
    // raw 是 VC 之后还没解密的原始字节. 只对 method+padD 做 RC4, 多读的握手按 method 处理.
    async fn take_enc(
        read: &mut BoxAsyncReadVectored,
        raw: &mut Vec<u8>,
        enc_b: &mut Rc4,
        n: usize,
    ) -> anyhow::Result<Vec<u8>> {
        while raw.len() < n {
            let mut tmp = [0u8; 64];
            let k = read.read(&mut tmp).await.context("mse read select")?;
            if k == 0 {
                bail!("mse eof in select");
            }
            raw.extend_from_slice(&tmp[..k]);
        }
        let mut chunk: Vec<u8> = raw.drain(..n).collect();
        enc_b.xor(&mut chunk);
        Ok(chunk)
    }

    let method_b = take_enc(&mut read, &mut raw, &mut enc_b, 4).await?;
    let method = u32::from_be_bytes(method_b.try_into().unwrap());
    let pad_b = take_enc(&mut read, &mut raw, &mut enc_b, 2).await?;
    let pad_len = u16::from_be_bytes(pad_b.try_into().unwrap()) as usize;
    if pad_len > 512 {
        bail!("mse padD too large: {pad_len}");
    }
    let _ = take_enc(&mut read, &mut raw, &mut enc_b, pad_len).await?;
    tracing::debug!(method, leftover = raw.len(), pad_len, "mse crypto_select");

    let use_rc4 = method & CRYPTO_RC4 != 0;
    if !use_rc4 && method & CRYPTO_PLAIN == 0 {
        bail!("mse peer chose unsupported method {method:#x}");
    }

    // leftover: 明文模式已经是握手; RC4 模式还是密文, 需要继续 XOR
    let leftover = if use_rc4 {
        enc_b.xor(&mut raw);
        raw
    } else {
        raw
    };
    Ok(wrap_pair(
        read,
        write,
        leftover,
        use_rc4.then_some(enc_b),
        use_rc4.then_some(enc_a),
    ))
}

/// 入站: 先看 20 字节. 明文握手原样塞回; 否则当 MSE Ya 前缀.
pub async fn maybe_incoming(
    mut read: BoxAsyncReadVectored,
    write: BoxAsyncWrite,
    skeys: &[[u8; 20]],
) -> anyhow::Result<(BoxAsyncReadVectored, BoxAsyncWrite)> {
    let mut head = [0u8; 20];
    read.read_exact(&mut head).await.context("mse peek")?;
    if head[0] == 19 && &head[1..] == PSTR {
        return Ok((prepend(head.to_vec(), read), write));
    }
    if skeys.is_empty() {
        bail!("mse incoming, no torrents");
    }
    receive(read, write, skeys, &head).await
}

async fn receive(
    mut read: BoxAsyncReadVectored,
    mut write: BoxAsyncWrite,
    skeys: &[[u8; 20]],
    ya_prefix: &[u8],
) -> anyhow::Result<(BoxAsyncReadVectored, BoxAsyncWrite)> {
    let mut ya = [0u8; 96];
    if ya_prefix.len() > 96 {
        bail!("mse ya prefix too long");
    }
    ya[..ya_prefix.len()].copy_from_slice(ya_prefix);
    if ya_prefix.len() < 96 {
        read.read_exact(&mut ya[ya_prefix.len()..])
            .await
            .context("mse read Ya")?;
    }

    let xa = new_xa();
    let yb = dh_public(&xa);
    let mut pad_b = [0u8; 16];
    rand::rng().fill_bytes(&mut pad_b);
    let mut pkt = Vec::with_capacity(112);
    pkt.extend_from_slice(&yb);
    pkt.extend_from_slice(&pad_b);
    write.write_all(&pkt).await.context("mse write Yb")?;
    write.flush().await.context("mse flush Yb")?;

    let s = dh_secret(&ya, &xa);
    let req1 = sha1_parts(&[b"req1", &s]);

    let mut window = Vec::with_capacity(532);
    let mut tmp = [0u8; 64];
    loop {
        if window.len() >= 20 {
            if let Some(pos) = window.windows(20).position(|w| w == req1.as_slice()) {
                let rest = window.split_off(pos + 20);
                return finish_receive(read, write, skeys, s, rest).await;
            }
        }
        if window.len() >= 532 {
            bail!("mse: req1 not found");
        }
        let n = read.read(&mut tmp).await.context("mse read req1")?;
        if n == 0 {
            bail!("mse eof waiting req1");
        }
        window.extend_from_slice(&tmp[..n]);
        if window.len() > 532 {
            let drop = window.len() - 532;
            window.drain(..drop);
        }
    }
}

async fn finish_receive(
    mut read: BoxAsyncReadVectored,
    mut write: BoxAsyncWrite,
    skeys: &[[u8; 20]],
    s: [u8; 96],
    mut raw: Vec<u8>,
) -> anyhow::Result<(BoxAsyncReadVectored, BoxAsyncWrite)> {
    async fn take_raw(
        read: &mut BoxAsyncReadVectored,
        raw: &mut Vec<u8>,
        n: usize,
    ) -> anyhow::Result<Vec<u8>> {
        while raw.len() < n {
            let mut tmp = [0u8; 64];
            let k = read.read(&mut tmp).await.context("mse recv")?;
            if k == 0 {
                bail!("mse eof recv");
            }
            raw.extend_from_slice(&tmp[..k]);
        }
        Ok(raw.drain(..n).collect())
    }

    let xored = take_raw(&mut read, &mut raw, 20).await?;
    let req3 = sha1_parts(&[b"req3", &s]);
    let mut skey = None;
    for k in skeys {
        let expect: Vec<u8> = sha1_parts(&[b"req2", k])
            .iter()
            .zip(req3.iter())
            .map(|(a, b)| a ^ b)
            .collect();
        if expect == xored {
            skey = Some(*k);
            break;
        }
    }
    let skey = skey.context("mse no matching skey")?;

    let mut enc_a = Rc4::new(&sha1_parts(&[b"keyA", &s, &skey]));
    let mut enc_b = Rc4::new(&sha1_parts(&[b"keyB", &s, &skey]));

    async fn take_enc(
        read: &mut BoxAsyncReadVectored,
        raw: &mut Vec<u8>,
        enc: &mut Rc4,
        n: usize,
    ) -> anyhow::Result<Vec<u8>> {
        while raw.len() < n {
            let mut tmp = [0u8; 64];
            let k = read.read(&mut tmp).await.context("mse recv enc")?;
            if k == 0 {
                bail!("mse eof recv enc");
            }
            raw.extend_from_slice(&tmp[..k]);
        }
        let mut chunk: Vec<u8> = raw.drain(..n).collect();
        enc.xor(&mut chunk);
        Ok(chunk)
    }

    let vc = take_enc(&mut read, &mut raw, &mut enc_a, 8).await?;
    if vc.iter().any(|b| *b != 0) {
        bail!("mse bad VC");
    }
    let provide_b = take_enc(&mut read, &mut raw, &mut enc_a, 4).await?;
    let provide = u32::from_be_bytes(provide_b.try_into().unwrap());
    let pad_b = take_enc(&mut read, &mut raw, &mut enc_a, 2).await?;
    let pad_len = u16::from_be_bytes(pad_b.try_into().unwrap()) as usize;
    if pad_len > 512 {
        bail!("mse padC too large: {pad_len}");
    }
    let _ = take_enc(&mut read, &mut raw, &mut enc_a, pad_len).await?;
    let ia_len_b = take_enc(&mut read, &mut raw, &mut enc_a, 2).await?;
    let ia_len = u16::from_be_bytes(ia_len_b.try_into().unwrap()) as usize;
    let ia = take_enc(&mut read, &mut raw, &mut enc_a, ia_len).await?;

    // 两边都给时跟迅雷一样选明文; 只有 RC4 才包流
    let use_rc4 = provide & CRYPTO_PLAIN == 0 && provide & CRYPTO_RC4 != 0;
    let method = if use_rc4 { CRYPTO_RC4 } else { CRYPTO_PLAIN };
    tracing::debug!(provide, method, ia_len, leftover = raw.len(), "mse incoming");

    let mut pad_d = [0u8; 8];
    rand::rng().fill_bytes(&mut pad_d);
    let mut body = Vec::new();
    body.extend_from_slice(&[0u8; 8]);
    body.extend_from_slice(&method.to_be_bytes());
    body.extend_from_slice(&(pad_d.len() as u16).to_be_bytes());
    body.extend_from_slice(&pad_d);
    let enc_body = enc_b.crypt(&body);
    write.write_all(&enc_body).await.context("mse write select")?;
    write.flush().await.context("mse flush select")?;

    let leftover = if use_rc4 {
        enc_a.xor(&mut raw);
        let mut o = ia;
        o.append(&mut raw);
        o
    } else {
        let mut o = ia;
        o.append(&mut raw);
        o
    };
    Ok(wrap_pair(
        read,
        write,
        leftover,
        use_rc4.then_some(enc_a),
        use_rc4.then_some(enc_b),
    ))
}

struct PrefixRead {
    prefix: Vec<u8>,
    prefix_pos: usize,
    inner: BoxAsyncReadVectored,
}

impl AsyncRead for PrefixRead {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let this = self.as_mut().get_mut();
        if this.prefix_pos < this.prefix.len() {
            let n = (this.prefix.len() - this.prefix_pos).min(buf.remaining());
            buf.put_slice(&this.prefix[this.prefix_pos..this.prefix_pos + n]);
            this.prefix_pos += n;
            return Poll::Ready(Ok(()));
        }
        Pin::new(&mut this.inner).poll_read(cx, buf)
    }
}

/// 给 dd-core 单测用: 本机 TCP 上走完 initiate/receive, 再互发 14 字节.
pub async fn self_check() -> anyhow::Result<()> {
    use tokio::net::TcpListener;
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    let skey = [7u8; 20];
    let server = tokio::spawn(async move {
        let (s, _) = listener.accept().await?;
        let _ = s.set_nodelay(true);
        let (r, w) = s.into_split();
        let r: BoxAsyncReadVectored = Box::new(r.into_vectored_compat());
        let w: BoxAsyncWrite = Box::new(w);
        maybe_incoming(r, w, &[skey]).await
    });
    let client = tokio::net::TcpStream::connect(addr).await?;
    let _ = client.set_nodelay(true);
    let (r, w) = client.into_split();
    let r: BoxAsyncReadVectored = Box::new(r.into_vectored_compat());
    let w: BoxAsyncWrite = Box::new(w);
    let (mut cr, mut cw) = initiate(r, w, &skey).await?;
    let (mut sr, mut sw) = server.await.context("server join")??;

    cw.write_all(b"ping-from-init").await?;
    cw.flush().await?;
    let mut buf = [0u8; 14];
    sr.read_exact(&mut buf).await?;
    anyhow::ensure!(&buf == b"ping-from-init", "init->recv {:?}", buf);

    sw.write_all(b"pong-from-recv").await?;
    sw.flush().await?;
    cr.read_exact(&mut buf).await?;
    anyhow::ensure!(&buf == b"pong-from-recv", "recv->init {:?}", buf);
    Ok(())
}


