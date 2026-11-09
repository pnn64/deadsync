//! The existing observational source stream, separate from message delivery.
use super::*;
use deadlib_lua::{NativeDebugFrame, NativeStack};
use serde_json::{Value as Json, json};
type Output = Arc<Mutex<Option<(bool, Result<Option<Json>, String>)>>>;

pub(super) fn capture(
    call: &mut HostCall<'_>,
    store: &Shared,
    name: &[u8],
    params: Option<Value>,
) -> Result<(), Error> {
    if store.lock().unwrap().live_sources.is_none() {
        return Ok(());
    }
    let output: Output = Arc::new(Mutex::new(None));
    let run = call.new_host_function_with_data(
        |call| {
            let params = call.captures()[0];
            let omitted = call.captures()[1] == Value::Boolean(true);
            let output = call.function_data::<Output>().unwrap().clone();
            let mut s = call.native_stack()?;
            let thread = s.thread_id();
            let mut eligible = true;
            for level in 1..32 {
                let Some(frame) = s.get_stack(thread, level)? else {
                    break;
                };
                let info = s.get_info(frame, b"S")?;
                if info.what != Some(b"C\0".as_slice()) {
                    if let Some(source) = info.source.and_then(|source| s.debug_bytes(source)) {
                        eligible = !source.starts_with(b"@theme:");
                    }
                    break;
                }
            }
            if !eligible {
                *output.lock().unwrap() = Some((false, Ok(None)));
                return Ok(());
            }
            let result = if omitted {
                Ok(None)
            } else if params == Value::Nil {
                Ok(Some(json!({"type":"nil"})))
            } else {
                s.push(params)?;
                let value = encode(&mut s, 0, &mut Vec::new());
                s.set_top(0)?;
                value.map(Some)
            };
            *output.lock().unwrap() = Some((eligible, result));
            Ok(())
        },
        &[
            params.unwrap_or(Value::Nil),
            Value::Boolean(params.is_none()),
        ],
        output.clone(),
    )?;
    call.call(run, &[])?;
    let (eligible, params) = output.lock().unwrap().take().unwrap();
    if !eligible {
        return Ok(());
    }
    let [beat, music] = effects::values(call, false).unwrap_or([0.; 2]);
    if let Some(rows) = &mut store.lock().unwrap().live_sources {
        rows.push(crate::SongLuaLiveBroadcast {
            beat,
            music_seconds: music as f64,
            name: String::from_utf8_lossy(name).into_owned(),
            params,
        });
    }
    Ok(())
}
fn number(n: f64) -> Json {
    if n == (n as u32) as f64 {
        json!(n as u32)
    } else if n == (n as i32) as f64 {
        json!(n as i32)
    } else {
        json!(n)
    }
}
fn encode(
    s: &mut NativeStack<'_, '_>,
    depth: usize,
    seen: &mut Vec<usize>,
) -> Result<Json, String> {
    let value = s.value(-1).unwrap();
    let index = s.top_offset();
    Ok(match value {
        Value::Nil => Json::Null,
        Value::Boolean(b) => json!(b),
        Value::Number(n) if n.is_finite() => number(n),
        Value::String(_) => Json::String(
            std::str::from_utf8(s.to_bytes(-1).map_err(|e| e.to_string())?.unwrap())
                .map_err(|e| e.to_string())?
                .replace('\\', "/"),
        ),
        Value::Native(_) | Value::Function(_) => {
            s.push(value).map_err(|e| e.to_string())?;
            let info = s
                .get_info(NativeDebugFrame::from_index(s.thread_id(), 0), b">S")
                .map_err(|e| e.to_string())?;
            let source = info
                .source
                .and_then(|source| s.debug_bytes(source))
                .unwrap_or_default();
            let source = source.strip_suffix(&[0]).unwrap_or(source);
            let [line, last] = info.defined.unwrap_or([-1, -1]);
            json!({"type":"function","source":String::from_utf8_lossy(source).replace('\\',"/"),"line":line,"last_line":last})
        }
        Value::Table(_) => {
            let identity = s.to_pointer(-1).unwrap();
            if depth >= 5 {
                return Err("broadcast table exceeds native trace depth".into());
            }
            if seen.contains(&identity) {
                return Err("broadcast table contains a cycle".into());
            }
            for key in [b"GetName".as_slice(), b"GetParent"] {
                s.push_bytes(key).map_err(|e| e.to_string())?;
                s.get_table(index, true).map_err(|e| e.to_string())?;
                let actor = matches!(s.value(-1), Some(Value::Function(_) | Value::Native(_)));
                s.set_top(s.top() - 1).map_err(|e| e.to_string())?;
                if actor {
                    return Err("broadcast actor identity remains pending".into());
                }
            }
            seen.push(identity);
            let result = table_json(s, index, depth, seen);
            seen.pop();
            result?
        }
        Value::Number(_) => return Err("broadcast number trace remains pending".into()),
        value => {
            return Err(format!(
                "broadcast {} trace remains pending",
                value.type_name()
            ));
        }
    })
}
fn table_json(
    s: &mut NativeStack<'_, '_>,
    index: isize,
    depth: usize,
    seen: &mut Vec<usize>,
) -> Result<Json, String> {
    let mut entries = Vec::new();
    let mut array = true;
    let mut max = 0;
    s.push(Value::Nil).map_err(|e| e.to_string())?;
    while s.next_table(index).map_err(|e| e.to_string())? {
        if entries.len() == 256 {
            return Err("broadcast table exceeds native trace entry limit".into());
        }
        let (name, ix) = match s.value(-2).unwrap() {
            Value::String(_) => (
                std::str::from_utf8(s.to_bytes(-2).map_err(|e| e.to_string())?.unwrap())
                    .map_err(|e| e.to_string())?
                    .to_owned(),
                None,
            ),
            Value::Boolean(b) => (b.to_string(), None),
            Value::Number(n) if n.is_finite() && n.fract() == 0. => {
                (n.to_string(), (n > 0. && n <= 256.).then_some(n as usize))
            }
            _ => return Err("broadcast table key representation remains pending".into()),
        };
        array &= ix.is_some();
        max = max.max(ix.unwrap_or(0));
        entries.push((name, ix, encode(s, depth + 1, seen)?));
        s.set_top(s.top() - 1).map_err(|e| e.to_string())?;
    }
    if array && max == entries.len() {
        let mut a = vec![Json::Null; entries.len()];
        for (_, i, v) in entries {
            a[i.unwrap() - 1] = v;
        }
        Ok(Json::Array(a))
    } else {
        let mut map = serde_json::Map::new();
        for (k, _, v) in entries {
            if map.insert(k, v).is_some() {
                return Err("broadcast table has colliding stringified keys".into());
            }
        }
        Ok(Json::Object(map))
    }
}
