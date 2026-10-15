//! Reading what a weights file declares about itself, without loading it.
//!
//! Two formats, both read from their headers only:
//!
//! - **Safetensors**: an 8-byte little-endian header length, then a JSON
//!   header naming every tensor's dtype, shape and byte range. The tensor
//!   data that follows is never read.
//! - **ONNX**: a protobuf `ModelProto`. ONNX files carry their weights
//!   inline (`initializer` tensors' `raw_data`), so a model can be gigabytes;
//!   the reader streams the message and *seeks past* every bulk field, so
//!   memory stays bounded by the graph's structure, not its weights.
//!
//! Nothing here runs a model or links a runtime. The protobuf reader knows
//! only the handful of ONNX fields Epoch shows (ir/opset versions, producer,
//! graph inputs/outputs with shapes, initializer names/dtypes/dims, operator
//! counts) and skips every other field by its wire type, which is the
//! protobuf forward-compatibility rule.

use crate::error::{CoreError, Result};
use crate::paths::ProjectRoot;
use serde::Serialize;
use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::Path;

/// Safetensors caps its own header at 100 MB; so does Epoch.
const MAX_SAFETENSORS_HEADER: u64 = 100 * 1024 * 1024;
/// A single protobuf sub-message read into memory (a node, a value info).
/// Anything larger is a weight blob and is skipped instead.
const MAX_INLINE_MESSAGE: u64 = 16 * 1024 * 1024;
/// Tensors listed individually; parameter totals still count every one.
pub const MAX_LISTED_TENSORS: usize = 4000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelFormat {
    Safetensors,
    Onnx,
}

impl ModelFormat {
    pub fn from_path(path: &Path) -> Option<Self> {
        let ext = path.extension()?.to_string_lossy().to_ascii_lowercase();
        match ext.as_str() {
            "safetensors" => Some(Self::Safetensors),
            "onnx" => Some(Self::Onnx),
            _ => None,
        }
    }
}

/// One named tensor: a weight, or a graph input/output.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TensorInfo {
    pub name: String,
    pub dtype: String,
    /// Each dimension as text: a number, or a symbolic name (`batch`) or
    /// `?` for an unknown ONNX dimension.
    pub shape: Vec<String>,
    /// Element count, when every dimension is a number.
    pub elements: Option<u64>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct OpCount {
    pub op: String,
    pub count: u64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct OnnxInfo {
    pub ir_version: Option<i64>,
    /// `(domain, version)`; the default domain is shown as `ai.onnx`.
    pub opsets: Vec<(String, i64)>,
    pub producer: Option<String>,
    pub graph_name: Option<String>,
    pub inputs: Vec<TensorInfo>,
    pub outputs: Vec<TensorInfo>,
    pub node_count: u64,
    /// Operator types by frequency, most used first.
    pub ops: Vec<OpCount>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ModelFileInfo {
    pub path: String,
    pub format: ModelFormat,
    pub size: u64,
    /// Weights (Safetensors tensors, ONNX initializers), in file order.
    pub tensors: Vec<TensorInfo>,
    pub tensor_count: u64,
    /// Sum of element counts over tensors whose shape is fully known.
    pub parameters: u64,
    /// Parameter count per dtype, largest first.
    pub dtypes: Vec<(String, u64)>,
    /// Safetensors `__metadata__`, ONNX `metadata_props`.
    pub metadata: BTreeMap<String, String>,
    pub onnx: Option<OnnxInfo>,
    /// More tensors than [`MAX_LISTED_TENSORS`]; the list is cut, totals aren't.
    pub truncated: bool,
}

/// Inspect a `.safetensors` or `.onnx` file inside the project.
pub fn inspect(root: &ProjectRoot, rel: &str) -> Result<ModelFileInfo> {
    let path = root.resolve(rel)?;
    let format = ModelFormat::from_path(&path)
        .ok_or_else(|| CoreError::Invalid(format!("{rel}: not a .safetensors or .onnx file")))?;
    let file = File::open(&path).map_err(|e| CoreError::io(&path, e))?;
    let size = file.metadata().map_err(|e| CoreError::io(&path, e))?.len();
    let mut reader = BufReader::with_capacity(64 * 1024, file);
    let mut info = match format {
        ModelFormat::Safetensors => read_safetensors(&mut reader, size),
        ModelFormat::Onnx => read_onnx(&mut reader, size),
    }
    .map_err(|e| match e {
        ReadError::Io(source) => CoreError::io(&path, source),
        ReadError::Format(msg) => CoreError::Invalid(format!("{rel}: {msg}")),
    })?;
    info.path = rel.to_owned();
    Ok(info)
}

#[derive(Debug)]
enum ReadError {
    Io(std::io::Error),
    Format(String),
}

impl From<std::io::Error> for ReadError {
    fn from(e: std::io::Error) -> Self {
        if e.kind() == std::io::ErrorKind::UnexpectedEof {
            Self::Format("file ends early".into())
        } else {
            Self::Io(e)
        }
    }
}

fn bad<T>(msg: impl Into<String>) -> std::result::Result<T, ReadError> {
    Err(ReadError::Format(msg.into()))
}

type R<T> = std::result::Result<T, ReadError>;

fn elements_of(dims: &[Option<u64>]) -> Option<u64> {
    dims.iter()
        .try_fold(1u64, |acc, d| d.and_then(|d| acc.checked_mul(d)))
}

/// Accumulates the weight list and its totals for either format.
#[derive(Default)]
struct Weights {
    tensors: Vec<TensorInfo>,
    count: u64,
    parameters: u64,
    by_dtype: BTreeMap<String, u64>,
}

impl Weights {
    fn push(&mut self, name: String, dtype: String, dims: &[Option<u64>], shape: Vec<String>) {
        let elements = elements_of(dims);
        self.count += 1;
        if let Some(n) = elements {
            self.parameters = self.parameters.saturating_add(n);
            *self.by_dtype.entry(dtype.clone()).or_default() += n;
        }
        if self.tensors.len() < MAX_LISTED_TENSORS {
            self.tensors.push(TensorInfo {
                name,
                dtype,
                shape,
                elements,
            });
        }
    }

    fn finish(self, format: ModelFormat, size: u64) -> ModelFileInfo {
        let mut dtypes: Vec<(String, u64)> = self.by_dtype.into_iter().collect();
        dtypes.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        ModelFileInfo {
            path: String::new(),
            format,
            size,
            truncated: self.count as usize > self.tensors.len(),
            tensors: self.tensors,
            tensor_count: self.count,
            parameters: self.parameters,
            dtypes,
            metadata: BTreeMap::new(),
            onnx: None,
        }
    }
}

// ── Safetensors ────────────────────────────────────────────────────────

fn read_safetensors<Rd: Read>(r: &mut Rd, size: u64) -> R<ModelFileInfo> {
    let mut len = [0u8; 8];
    r.read_exact(&mut len)?;
    let header_len = u64::from_le_bytes(len);
    if header_len > MAX_SAFETENSORS_HEADER || header_len > size.saturating_sub(8) {
        return bad(format!("header length {header_len} is not plausible"));
    }
    let mut header = vec![0u8; header_len as usize];
    r.read_exact(&mut header)?;
    let json: serde_json::Value = serde_json::from_slice(&header)
        .map_err(|e| ReadError::Format(format!("header is not JSON: {e}")))?;
    let serde_json::Value::Object(entries) = json else {
        return bad("header is not a JSON object");
    };
    let data_len = size - 8 - header_len;
    let mut weights = Weights::default();
    let mut metadata = BTreeMap::new();
    // serde_json's map is ordered by key; Safetensors writers order tensors
    // by data offset, which reads as file order, so sort by that.
    let mut ordered: Vec<(u64, String, serde_json::Value)> = Vec::new();
    for (name, value) in entries {
        if name == "__metadata__" {
            if let serde_json::Value::Object(m) = value {
                for (k, v) in m {
                    let text = v
                        .as_str()
                        .map(str::to_owned)
                        .unwrap_or_else(|| v.to_string());
                    metadata.insert(k, text);
                }
            }
            continue;
        }
        let offsets = value.get("data_offsets").and_then(|o| o.as_array());
        let (start, end) = match offsets.map(|o| {
            (
                o.first().and_then(|v| v.as_u64()),
                o.get(1).and_then(|v| v.as_u64()),
            )
        }) {
            Some((Some(s), Some(e))) => (s, e),
            _ => return bad(format!("tensor {name} has no data_offsets")),
        };
        if start > end || end > data_len {
            return bad(format!("tensor {name} points outside the file"));
        }
        ordered.push((start, name, value));
    }
    ordered.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
    for (_, name, value) in ordered {
        let dtype = value
            .get("dtype")
            .and_then(|d| d.as_str())
            .unwrap_or("?")
            .to_owned();
        let dims: Vec<Option<u64>> = value
            .get("shape")
            .and_then(|s| s.as_array())
            .map(|s| s.iter().map(serde_json::Value::as_u64).collect())
            .unwrap_or_default();
        let shape = dims
            .iter()
            .map(|d| d.map_or_else(|| "?".to_owned(), |d| d.to_string()))
            .collect();
        weights.push(name, dtype, &dims, shape);
    }
    let mut info = weights.finish(ModelFormat::Safetensors, size);
    info.metadata = metadata;
    Ok(info)
}

// ── ONNX (protobuf) ───────────────────────────────────────────────────

const WIRE_VARINT: u8 = 0;
const WIRE_I64: u8 = 1;
const WIRE_LEN: u8 = 2;
const WIRE_I32: u8 = 5;

/// A streaming protobuf reader that tracks its own position.
struct Stream<'a, Rd: Read + Seek> {
    r: &'a mut Rd,
    pos: u64,
}

impl<Rd: Read + Seek> Stream<'_, Rd> {
    fn byte(&mut self) -> R<u8> {
        let mut b = [0u8; 1];
        self.r.read_exact(&mut b)?;
        self.pos += 1;
        Ok(b[0])
    }

    fn varint(&mut self) -> R<u64> {
        let mut out = 0u64;
        for shift in (0..64).step_by(7) {
            let b = self.byte()?;
            out |= u64::from(b & 0x7f) << shift;
            if b & 0x80 == 0 {
                return Ok(out);
            }
        }
        bad("malformed varint")
    }

    fn skip(&mut self, n: u64) -> R<()> {
        self.r.seek(SeekFrom::Current(n as i64))?;
        self.pos += n;
        Ok(())
    }

    fn bytes(&mut self, n: u64) -> R<Vec<u8>> {
        if n > MAX_INLINE_MESSAGE {
            return bad(format!(
                "a {n}-byte field where a small message was expected"
            ));
        }
        let mut buf = vec![0u8; n as usize];
        self.r.read_exact(&mut buf)?;
        self.pos += n;
        Ok(buf)
    }

    /// The next field header, or None at `end`.
    fn field(&mut self, end: u64) -> R<Option<(u32, u8)>> {
        if self.pos >= end {
            return Ok(None);
        }
        let key = self.varint()?;
        Ok(Some(((key >> 3) as u32, (key & 7) as u8)))
    }

    fn skip_value(&mut self, wire: u8, end: u64) -> R<()> {
        match wire {
            WIRE_VARINT => {
                self.varint()?;
            }
            WIRE_I64 => self.skip(8)?,
            WIRE_I32 => self.skip(4)?,
            WIRE_LEN => {
                let n = self.varint()?;
                if self.pos + n > end {
                    return bad("field runs past its message");
                }
                self.skip(n)?;
            }
            _ => return bad(format!("unsupported wire type {wire}")),
        }
        Ok(())
    }

    /// The bounds of a length-delimited field that starts here.
    fn sub(&mut self, end: u64) -> R<u64> {
        let n = self.varint()?;
        let sub_end = self.pos + n;
        if sub_end > end {
            return bad("message runs past its parent");
        }
        Ok(sub_end)
    }
}

/// An in-memory protobuf message (small ones only: nodes, value infos).
struct Slice<'a> {
    buf: &'a [u8],
    at: usize,
}

enum Value<'a> {
    Int(u64),
    Bytes(&'a [u8]),
    Fixed,
}

impl<'a> Slice<'a> {
    fn new(buf: &'a [u8]) -> Self {
        Self { buf, at: 0 }
    }

    fn varint(&mut self) -> R<u64> {
        let mut out = 0u64;
        for shift in (0..64).step_by(7) {
            let Some(&b) = self.buf.get(self.at) else {
                return bad("message ends inside a varint");
            };
            self.at += 1;
            out |= u64::from(b & 0x7f) << shift;
            if b & 0x80 == 0 {
                return Ok(out);
            }
        }
        bad("malformed varint")
    }

    fn next(&mut self) -> R<Option<(u32, Value<'a>)>> {
        if self.at >= self.buf.len() {
            return Ok(None);
        }
        let key = self.varint()?;
        let (field, wire) = ((key >> 3) as u32, (key & 7) as u8);
        let value = match wire {
            WIRE_VARINT => Value::Int(self.varint()?),
            WIRE_LEN => {
                let n = self.varint()? as usize;
                let end = self.at.checked_add(n).filter(|&e| e <= self.buf.len());
                let Some(end) = end else {
                    return bad("field runs past its message");
                };
                let bytes = &self.buf[self.at..end];
                self.at = end;
                Value::Bytes(bytes)
            }
            WIRE_I64 | WIRE_I32 => {
                let n = if wire == WIRE_I64 { 8 } else { 4 };
                if self.at + n > self.buf.len() {
                    return bad("message ends inside a fixed field");
                }
                self.at += n;
                Value::Fixed
            }
            _ => return bad(format!("unsupported wire type {wire}")),
        };
        Ok(Some((field, value)))
    }
}

fn text(b: &[u8]) -> String {
    String::from_utf8_lossy(b).into_owned()
}

/// ONNX `TensorProto.DataType` names, as the ONNX spec spells them.
fn onnx_dtype(code: u64) -> String {
    let name = match code {
        1 => "float32",
        2 => "uint8",
        3 => "int8",
        4 => "uint16",
        5 => "int16",
        6 => "int32",
        7 => "int64",
        8 => "string",
        9 => "bool",
        10 => "float16",
        11 => "float64",
        12 => "uint32",
        13 => "uint64",
        14 => "complex64",
        15 => "complex128",
        16 => "bfloat16",
        17 => "float8e4m3fn",
        18 => "float8e4m3fnuz",
        19 => "float8e5m2",
        20 => "float8e5m2fnuz",
        21 => "uint4",
        22 => "int4",
        23 => "float4e2m1",
        _ => return format!("type {code}"),
    };
    name.to_owned()
}

/// `ValueInfoProto` → name, element type and shape.
fn value_info(buf: &[u8]) -> R<TensorInfo> {
    let mut name = String::new();
    let mut dtype = "?".to_owned();
    let mut dims: Vec<Option<u64>> = Vec::new();
    let mut shape: Vec<String> = Vec::new();
    let mut m = Slice::new(buf);
    while let Some((f, v)) = m.next()? {
        match (f, v) {
            (1, Value::Bytes(b)) => name = text(b),
            // TypeProto.tensor_type
            (2, Value::Bytes(ty)) => {
                let mut t = Slice::new(ty);
                while let Some((f, v)) = t.next()? {
                    let (1, Value::Bytes(tensor)) = (f, v) else {
                        continue;
                    };
                    let mut tt = Slice::new(tensor);
                    while let Some((f, v)) = tt.next()? {
                        match (f, v) {
                            (1, Value::Int(code)) => dtype = onnx_dtype(code),
                            (2, Value::Bytes(shape_buf)) => {
                                let mut s = Slice::new(shape_buf);
                                while let Some((f, v)) = s.next()? {
                                    let (1, Value::Bytes(dim)) = (f, v) else {
                                        continue;
                                    };
                                    let mut d = Slice::new(dim);
                                    let mut label = "?".to_owned();
                                    let mut value = None;
                                    while let Some((f, v)) = d.next()? {
                                        match (f, v) {
                                            (1, Value::Int(n)) => {
                                                value = Some(n);
                                                label = n.to_string();
                                            }
                                            (2, Value::Bytes(p)) if !p.is_empty() => {
                                                label = text(p)
                                            }
                                            _ => {}
                                        }
                                    }
                                    dims.push(value);
                                    shape.push(label);
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
            _ => {}
        }
    }
    Ok(TensorInfo {
        name,
        dtype,
        elements: elements_of(&dims),
        shape,
    })
}

/// `NodeProto` → its operator type (field 4).
fn node_op(buf: &[u8]) -> R<String> {
    let mut m = Slice::new(buf);
    while let Some((f, v)) = m.next()? {
        if let (4, Value::Bytes(b)) = (f, v) {
            return Ok(text(b));
        }
    }
    Ok("?".into())
}

/// A packed or unpacked repeated int64 field, appended to `out`.
fn read_dims<Rd: Read + Seek>(
    s: &mut Stream<'_, Rd>,
    wire: u8,
    end: u64,
    out: &mut Vec<Option<u64>>,
) -> R<()> {
    match wire {
        WIRE_VARINT => out.push(Some(s.varint()?)),
        WIRE_LEN => {
            let sub_end = s.sub(end)?;
            while s.pos < sub_end {
                out.push(Some(s.varint()?));
            }
        }
        _ => s.skip_value(wire, end)?,
    }
    Ok(())
}

/// `TensorProto` (an initializer), streamed: dims (1), data_type (2) and
/// name (8) are read; the data fields are seeked past.
fn read_initializer<Rd: Read + Seek>(
    s: &mut Stream<'_, Rd>,
    end: u64,
    weights: &mut Weights,
) -> R<()> {
    let mut dims = Vec::new();
    let mut dtype = "?".to_owned();
    let mut name = String::new();
    while let Some((f, wire)) = s.field(end)? {
        match (f, wire) {
            (1, _) => read_dims(s, wire, end, &mut dims)?,
            (2, WIRE_VARINT) => dtype = onnx_dtype(s.varint()?),
            (8, WIRE_LEN) => {
                let n = s.varint()?;
                name = text(&s.bytes(n)?);
            }
            _ => s.skip_value(wire, end)?,
        }
    }
    let shape = dims
        .iter()
        .map(|d| d.map_or_else(|| "?".to_owned(), |d| d.to_string()))
        .collect();
    weights.push(name, dtype, &dims, shape);
    Ok(())
}

fn read_onnx<Rd: Read + Seek>(r: &mut Rd, size: u64) -> R<ModelFileInfo> {
    let mut s = Stream { r, pos: 0 };
    let mut weights = Weights::default();
    let mut metadata = BTreeMap::new();
    let mut onnx = OnnxInfo {
        ir_version: None,
        opsets: Vec::new(),
        producer: None,
        graph_name: None,
        inputs: Vec::new(),
        outputs: Vec::new(),
        node_count: 0,
        ops: Vec::new(),
    };
    let mut ops: BTreeMap<String, u64> = BTreeMap::new();
    let mut initializer_names: Vec<String> = Vec::new();
    let mut saw_graph = false;
    let mut producer_version = None;

    while let Some((f, wire)) = s.field(size)? {
        match (f, wire) {
            (1, WIRE_VARINT) => onnx.ir_version = Some(s.varint()? as i64),
            (2, WIRE_LEN) => {
                let n = s.varint()?;
                onnx.producer = Some(text(&s.bytes(n)?));
            }
            (3, WIRE_LEN) => {
                let n = s.varint()?;
                producer_version = Some(text(&s.bytes(n)?));
            }
            (7, WIRE_LEN) => {
                saw_graph = true;
                let graph_end = s.sub(size)?;
                while let Some((g, gw)) = s.field(graph_end)? {
                    match (g, gw) {
                        (1, WIRE_LEN) => {
                            let n = s.varint()?;
                            let op = node_op(&s.bytes(n)?)?;
                            onnx.node_count += 1;
                            *ops.entry(op).or_default() += 1;
                        }
                        (2, WIRE_LEN) => {
                            let n = s.varint()?;
                            onnx.graph_name = Some(text(&s.bytes(n)?));
                        }
                        (5, WIRE_LEN) => {
                            let end = s.sub(graph_end)?;
                            let before = weights.tensors.len();
                            read_initializer(&mut s, end, &mut weights)?;
                            if let Some(t) = weights.tensors.get(before) {
                                initializer_names.push(t.name.clone());
                            }
                        }
                        (11, WIRE_LEN) => {
                            let n = s.varint()?;
                            onnx.inputs.push(value_info(&s.bytes(n)?)?);
                        }
                        (12, WIRE_LEN) => {
                            let n = s.varint()?;
                            onnx.outputs.push(value_info(&s.bytes(n)?)?);
                        }
                        _ => s.skip_value(gw, graph_end)?,
                    }
                }
            }
            (8, WIRE_LEN) => {
                let n = s.varint()?;
                let buf = s.bytes(n)?;
                let mut m = Slice::new(&buf);
                let (mut domain, mut version) = (String::new(), 0i64);
                while let Some((f, v)) = m.next()? {
                    match (f, v) {
                        (1, Value::Bytes(b)) => domain = text(b),
                        (2, Value::Int(n)) => version = n as i64,
                        _ => {}
                    }
                }
                if domain.is_empty() {
                    domain = "ai.onnx".into();
                }
                onnx.opsets.push((domain, version));
            }
            (14, WIRE_LEN) => {
                let n = s.varint()?;
                let buf = s.bytes(n)?;
                let mut m = Slice::new(&buf);
                let (mut key, mut value) = (String::new(), String::new());
                while let Some((f, v)) = m.next()? {
                    match (f, v) {
                        (1, Value::Bytes(b)) => key = text(b),
                        (2, Value::Bytes(b)) => value = text(b),
                        _ => {}
                    }
                }
                metadata.insert(key, value);
            }
            _ => s.skip_value(wire, size)?,
        }
    }
    if !saw_graph && onnx.ir_version.is_none() {
        return bad("not an ONNX model (no graph or IR version)");
    }
    if let (Some(p), Some(v)) = (&onnx.producer, producer_version) {
        if !v.is_empty() {
            onnx.producer = Some(format!("{p} {v}"));
        }
    }
    // Older exporters list every initializer as a graph input too; those
    // are weights, not the inputs a caller feeds.
    if !initializer_names.is_empty() {
        let weights_named: std::collections::HashSet<&str> =
            initializer_names.iter().map(String::as_str).collect();
        onnx.inputs
            .retain(|i| !weights_named.contains(i.name.as_str()));
    }
    let mut op_list: Vec<OpCount> = ops
        .into_iter()
        .map(|(op, count)| OpCount { op, count })
        .collect();
    op_list.sort_by(|a, b| b.count.cmp(&a.count).then(a.op.cmp(&b.op)));
    onnx.ops = op_list;
    let mut info = weights.finish(ModelFormat::Onnx, size);
    info.metadata = metadata;
    info.onnx = Some(onnx);
    Ok(info)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn safetensors(header: &str, data: usize) -> Vec<u8> {
        let mut out = (header.len() as u64).to_le_bytes().to_vec();
        out.extend_from_slice(header.as_bytes());
        out.extend(std::iter::repeat_n(0u8, data));
        out
    }

    #[test]
    fn reads_a_safetensors_header() {
        let header = r#"{"__metadata__":{"format":"pt"},"b.bias":{"dtype":"F32","shape":[4],"data_offsets":[48,64]},"a.weight":{"dtype":"F32","shape":[4,3],"data_offsets":[0,48]}}"#;
        let bytes = safetensors(header, 64);
        let info = read_safetensors(&mut Cursor::new(&bytes), bytes.len() as u64).unwrap();
        assert_eq!(info.tensor_count, 2);
        assert_eq!(info.parameters, 16);
        // File order (by offset), not key order.
        assert_eq!(info.tensors[0].name, "a.weight");
        assert_eq!(info.tensors[0].shape, vec!["4", "3"]);
        assert_eq!(info.metadata.get("format").map(String::as_str), Some("pt"));
        assert_eq!(info.dtypes, vec![("F32".to_owned(), 16)]);
    }

    #[test]
    fn rejects_offsets_outside_the_file() {
        let header = r#"{"w":{"dtype":"F32","shape":[4],"data_offsets":[0,1000]}}"#;
        let bytes = safetensors(header, 16);
        assert!(read_safetensors(&mut Cursor::new(&bytes), bytes.len() as u64).is_err());
    }

    #[test]
    fn rejects_an_implausible_header_length() {
        let bytes = u64::MAX.to_le_bytes().to_vec();
        assert!(read_safetensors(&mut Cursor::new(&bytes), 8).is_err());
    }

    // A tiny protobuf writer for building ONNX fixtures.
    fn varint(mut v: u64, out: &mut Vec<u8>) {
        loop {
            let b = (v & 0x7f) as u8;
            v >>= 7;
            if v == 0 {
                out.push(b);
                return;
            }
            out.push(b | 0x80);
        }
    }
    fn int(field: u32, v: u64, out: &mut Vec<u8>) {
        varint(u64::from(field) << 3, out);
        varint(v, out);
    }
    fn len(field: u32, body: &[u8], out: &mut Vec<u8>) {
        varint((u64::from(field) << 3) | 2, out);
        varint(body.len() as u64, out);
        out.extend_from_slice(body);
    }
    fn value_info_msg(name: &str, elem: u64, dims: &[Result<u64, &str>]) -> Vec<u8> {
        let mut shape = Vec::new();
        for d in dims {
            let mut dim = Vec::new();
            match d {
                Ok(n) => int(1, *n, &mut dim),
                Err(p) => len(2, p.as_bytes(), &mut dim),
            }
            len(1, &dim, &mut shape);
        }
        let mut tensor = Vec::new();
        int(1, elem, &mut tensor);
        len(2, &shape, &mut tensor);
        let mut ty = Vec::new();
        len(1, &tensor, &mut ty);
        let mut vi = Vec::new();
        len(1, name.as_bytes(), &mut vi);
        len(2, &ty, &mut vi);
        vi
    }

    fn tiny_onnx() -> Vec<u8> {
        let mut graph = Vec::new();
        for op in ["Gemm", "Relu", "Gemm"] {
            let mut node = Vec::new();
            len(1, b"x", &mut node);
            len(4, op.as_bytes(), &mut node);
            len(1, &node, &mut graph);
        }
        len(2, b"mlp", &mut graph);
        // An initializer with packed dims and 48 bytes of raw data.
        let mut init = Vec::new();
        let mut packed = Vec::new();
        varint(3, &mut packed);
        varint(4, &mut packed);
        len(1, &packed, &mut init);
        int(2, 1, &mut init);
        len(8, b"fc.weight", &mut init);
        len(9, &[0u8; 48], &mut init);
        len(5, &init, &mut graph);
        // Unpacked dims.
        let mut bias = Vec::new();
        int(1, 4, &mut bias);
        int(2, 1, &mut bias);
        len(8, b"fc.bias", &mut bias);
        len(5, &bias, &mut graph);
        len(
            11,
            &value_info_msg("input", 1, &[Err("batch"), Ok(3)]),
            &mut graph,
        );
        // Old exporters repeat weights as inputs.
        len(11, &value_info_msg("fc.bias", 1, &[Ok(4)]), &mut graph);
        len(
            12,
            &value_info_msg("logits", 1, &[Err("batch"), Ok(4)]),
            &mut graph,
        );

        let mut model = Vec::new();
        int(1, 9, &mut model);
        len(2, b"pytorch", &mut model);
        len(3, b"2.4", &mut model);
        len(7, &graph, &mut model);
        let mut opset = Vec::new();
        int(2, 17, &mut opset);
        len(8, &opset, &mut model);
        let mut prop = Vec::new();
        len(1, b"task", &mut prop);
        len(2, b"classification", &mut prop);
        len(14, &prop, &mut model);
        model
    }

    #[test]
    fn reads_an_onnx_model() {
        let bytes = tiny_onnx();
        let info = read_onnx(&mut Cursor::new(&bytes), bytes.len() as u64).unwrap();
        let onnx = info.onnx.as_ref().unwrap();
        assert_eq!(onnx.ir_version, Some(9));
        assert_eq!(onnx.producer.as_deref(), Some("pytorch 2.4"));
        assert_eq!(onnx.graph_name.as_deref(), Some("mlp"));
        assert_eq!(onnx.opsets, vec![("ai.onnx".to_owned(), 17)]);
        assert_eq!(onnx.node_count, 3);
        assert_eq!(
            onnx.ops[0],
            OpCount {
                op: "Gemm".into(),
                count: 2
            }
        );
        assert_eq!(onnx.inputs.len(), 1, "weights listed as inputs are dropped");
        assert_eq!(onnx.inputs[0].shape, vec!["batch", "3"]);
        assert_eq!(onnx.inputs[0].elements, None);
        assert_eq!(onnx.outputs[0].name, "logits");
        assert_eq!(info.tensor_count, 2);
        assert_eq!(info.parameters, 16);
        assert_eq!(info.tensors[0].name, "fc.weight");
        assert_eq!(info.tensors[0].dtype, "float32");
        assert_eq!(
            info.metadata.get("task").map(String::as_str),
            Some("classification")
        );
    }

    #[test]
    fn truncated_onnx_is_an_error_not_a_panic() {
        let bytes = tiny_onnx();
        for cut in [1, 5, 20, bytes.len() / 2, bytes.len() - 1] {
            let part = &bytes[..cut];
            let _ = read_onnx(&mut Cursor::new(part), part.len() as u64);
        }
    }

    #[test]
    fn non_onnx_bytes_are_rejected() {
        let bytes = b"hello world, not a model".to_vec();
        assert!(read_onnx(&mut Cursor::new(&bytes), bytes.len() as u64).is_err());
    }

    #[test]
    fn inspect_resolves_inside_the_project() {
        let dir = tempfile::tempdir().unwrap();
        let header = r#"{"w":{"dtype":"BF16","shape":[2,2],"data_offsets":[0,8]}}"#;
        std::fs::write(dir.path().join("m.safetensors"), safetensors(header, 8)).unwrap();
        let root = ProjectRoot::open(dir.path()).unwrap();
        let info = inspect(&root, "m.safetensors").unwrap();
        assert_eq!(info.path, "m.safetensors");
        assert_eq!(info.parameters, 4);
        assert!(inspect(&root, "../m.safetensors").is_err());
        assert!(inspect(&root, "notes.txt").is_err());
    }
}

/// `EPOCH_MODEL_FILE=<abs path to .onnx/.safetensors> cargo test -p epoch-core real_model_file -- --nocapture`
#[cfg(test)]
#[test]
fn real_model_file() {
    let Ok(path) = std::env::var("EPOCH_MODEL_FILE") else {
        return;
    };
    let path = Path::new(&path);
    let root = ProjectRoot::open(path.parent().unwrap()).unwrap();
    let info = inspect(&root, &path.file_name().unwrap().to_string_lossy()).unwrap();
    println!("{}", serde_json::to_string_pretty(&info).unwrap());
}
