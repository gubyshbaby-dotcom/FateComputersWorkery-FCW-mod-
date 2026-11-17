//! JNI: класс `com.fatecomputers.fcw.client.nativelib.FcwNative` (CONTRACT.md §5).
//!
//! Хэндлы `long` — номера в реестре (не указатели), поэтому неверный или
//! закрытый хэндл безопасен. Ни одна паника не выходит за границу FFI, вызовы
//! не блокируют поток надолго (только короткие блокировки буферов).

use crate::glfw;
use crate::session::{Session, STATE_CLOSED};
use crate::view::View;
use jni::objects::{JByteBuffer, JClass, JIntArray, JShortArray, JString};
use jni::sys::{jboolean, jint, jlong, jstring, JNI_FALSE, JNI_TRUE};
use jni::JNIEnv;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, LazyLock};

type AnyResult<T> = Result<T, Box<dyn std::error::Error>>;

static NEXT_ID: AtomicI64 = AtomicI64::new(1);
static SESSIONS: LazyLock<Mutex<HashMap<i64, Arc<Session>>>> = LazyLock::new(|| Mutex::new(HashMap::new()));
static VIEWS: LazyLock<Mutex<HashMap<i64, (i64, Arc<View>)>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

fn guard<T>(default: T, f: impl FnOnce() -> AnyResult<T>) -> T {
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(Ok(v)) => v,
        _ => default,
    }
}

fn session(id: jlong) -> Option<Arc<Session>> {
    SESSIONS.lock().get(&id).cloned()
}

fn view(id: jlong) -> Option<Arc<View>> {
    VIEWS.lock().get(&id).map(|(_, v)| v.clone())
}

fn jstr(env: &mut JNIEnv, s: &JString) -> String {
    if s.is_null() {
        return String::new();
    }
    env.get_string(s).map(|s| s.into()).unwrap_or_default()
}

#[no_mangle]
pub extern "system" fn Java_com_fatecomputers_fcw_client_nativelib_FcwNative_connect<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
    host: JString<'l>,
    port: jint,
    cert_sha256_hex: JString<'l>,
) -> jlong {
    guard(0, || {
        let host = jstr(&mut env, &host);
        let pin = jstr(&mut env, &cert_sha256_hex);
        let s = Session::connect(&host, port.clamp(0, 65535) as u16, &pin);
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        SESSIONS.lock().insert(id, s);
        Ok(id)
    })
}

#[no_mangle]
pub extern "system" fn Java_com_fatecomputers_fcw_client_nativelib_FcwNative_sessionState<'l>(
    _env: JNIEnv<'l>,
    _class: JClass<'l>,
    session_id: jlong,
) -> jint {
    guard(STATE_CLOSED, || Ok(session(session_id).map(|s| s.state()).unwrap_or(STATE_CLOSED)))
}

#[no_mangle]
pub extern "system" fn Java_com_fatecomputers_fcw_client_nativelib_FcwNative_sessionError<'l>(
    env: JNIEnv<'l>,
    _class: JClass<'l>,
    session_id: jlong,
) -> jstring {
    guard(std::ptr::null_mut(), || {
        let msg = session(session_id).map(|s| s.error()).unwrap_or_else(|| "no such session".into());
        Ok(env.new_string(msg)?.into_raw())
    })
}

#[no_mangle]
pub extern "system" fn Java_com_fatecomputers_fcw_client_nativelib_FcwNative_close<'l>(
    _env: JNIEnv<'l>,
    _class: JClass<'l>,
    session_id: jlong,
) {
    guard((), || {
        let s = SESSIONS.lock().remove(&session_id);
        VIEWS.lock().retain(|_, (sid, _)| *sid != session_id);
        if let Some(s) = s {
            s.close();
        }
        Ok(())
    })
}

#[no_mangle]
pub extern "system" fn Java_com_fatecomputers_fcw_client_nativelib_FcwNative_attach<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
    session_id: jlong,
    token: JString<'l>,
) {
    guard((), || {
        let token = jstr(&mut env, &token);
        if let Some(s) = session(session_id) {
            s.attach(&token);
        }
        Ok(())
    })
}

#[no_mangle]
pub extern "system" fn Java_com_fatecomputers_fcw_client_nativelib_FcwNative_openView<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
    session_id: jlong,
    vm_id: JString<'l>,
    screen_id: JString<'l>,
    max_fps: jint,
) -> jlong {
    guard(0, || {
        let vm = jstr(&mut env, &vm_id);
        let screen = jstr(&mut env, &screen_id);
        let Some(s) = session(session_id) else { return Ok(0) };
        if vm.is_empty() || screen.is_empty() {
            return Ok(0);
        }
        let v = s.open_view(&vm, &screen, max_fps.max(0) as u32);
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        VIEWS.lock().insert(id, (session_id, v));
        Ok(id)
    })
}

#[no_mangle]
pub extern "system" fn Java_com_fatecomputers_fcw_client_nativelib_FcwNative_closeView<'l>(
    _env: JNIEnv<'l>,
    _class: JClass<'l>,
    view_id: jlong,
) {
    guard((), || {
        let entry = VIEWS.lock().remove(&view_id);
        if let Some((sid, v)) = entry {
            if let Some(s) = session(sid) {
                s.close_view(&v);
            }
        }
        Ok(())
    })
}

#[no_mangle]
pub extern "system" fn Java_com_fatecomputers_fcw_client_nativelib_FcwNative_viewInfo<'l>(
    env: JNIEnv<'l>,
    _class: JClass<'l>,
    view_id: jlong,
    out7: JIntArray<'l>,
) -> jboolean {
    guard(JNI_FALSE, || {
        let Some(v) = view(view_id) else { return Ok(JNI_FALSE) };
        if out7.is_null() {
            return Ok(JNI_FALSE);
        }
        let i = v.info();
        let vals = [
            i.width as jint,
            i.height as jint,
            i.kind,
            (i.seq & 0x7fff_ffff) as jint,
            i.cursor_visible as jint,
            i.cursor_x,
            i.cursor_y,
        ];
        let len = env.get_array_length(&out7)? as usize;
        let n = len.min(7);
        env.set_int_array_region(&out7, 0, &vals[..n])?;
        Ok(JNI_TRUE)
    })
}

#[no_mangle]
pub extern "system" fn Java_com_fatecomputers_fcw_client_nativelib_FcwNative_copyDirty<'l>(
    env: JNIEnv<'l>,
    _class: JClass<'l>,
    view_id: jlong,
    dst: JByteBuffer<'l>,
    rects: JIntArray<'l>,
) -> jint {
    guard(0, || {
        let Some(v) = view(view_id) else { return Ok(0) };
        if dst.is_null() {
            return Ok(0);
        }
        let ptr = env.get_direct_buffer_address(&dst)?;
        let cap = env.get_direct_buffer_capacity(&dst)?;
        if ptr.is_null() || cap == 0 {
            return Ok(0);
        }
        // SAFETY: direct-буфер Java живёт, пока жив объект `dst` (он удерживается
        // аргументом вызова); длина — его ёмкость.
        let buf = unsafe { std::slice::from_raw_parts_mut(ptr, cap) };
        let n = if rects.is_null() { 0 } else { env.get_array_length(&rects)? as usize };
        let mut r = vec![0i32; n];
        let res = v.copy_dirty(buf, &mut r);
        if res > 0 {
            env.set_int_array_region(&rects, 0, &r[..res as usize * 4])?;
        }
        Ok(res)
    })
}

#[no_mangle]
pub extern "system" fn Java_com_fatecomputers_fcw_client_nativelib_FcwNative_sendKey<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
    session_id: jlong,
    vm_id: JString<'l>,
    glfw_key: jint,
    glfw_scancode: jint,
    down: jboolean,
) {
    guard((), || {
        let vm = jstr(&mut env, &vm_id);
        if let (Some(s), Some(k)) = (session(session_id), glfw::to_evdev(glfw_key, glfw_scancode)) {
            s.send_key(&vm, k, down != JNI_FALSE);
        }
        Ok(())
    })
}

#[no_mangle]
pub extern "system" fn Java_com_fatecomputers_fcw_client_nativelib_FcwNative_sendText<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
    session_id: jlong,
    vm_id: JString<'l>,
    text: JString<'l>,
) {
    guard((), || {
        let vm = jstr(&mut env, &vm_id);
        let text = jstr(&mut env, &text);
        if let Some(s) = session(session_id) {
            s.send_text(&vm, &text);
        }
        Ok(())
    })
}

#[no_mangle]
pub extern "system" fn Java_com_fatecomputers_fcw_client_nativelib_FcwNative_sendPointer<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
    session_id: jlong,
    vm_id: JString<'l>,
    x: jint,
    y: jint,
) {
    guard((), || {
        let vm = jstr(&mut env, &vm_id);
        if let Some(s) = session(session_id) {
            s.send_pointer(&vm, x, y);
        }
        Ok(())
    })
}

#[no_mangle]
pub extern "system" fn Java_com_fatecomputers_fcw_client_nativelib_FcwNative_sendButtons<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
    session_id: jlong,
    vm_id: JString<'l>,
    mask: jint,
    x: jint,
    y: jint,
) {
    guard((), || {
        let vm = jstr(&mut env, &vm_id);
        if let Some(s) = session(session_id) {
            s.send_buttons(&vm, mask.max(0) as u32, x, y);
        }
        Ok(())
    })
}

#[no_mangle]
pub extern "system" fn Java_com_fatecomputers_fcw_client_nativelib_FcwNative_sendWheel<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
    session_id: jlong,
    vm_id: JString<'l>,
    dx: jint,
    dy: jint,
    x: jint,
    y: jint,
) {
    guard((), || {
        let vm = jstr(&mut env, &vm_id);
        if let Some(s) = session(session_id) {
            s.send_wheel(&vm, dx, dy, x, y);
        }
        Ok(())
    })
}

#[no_mangle]
pub extern "system" fn Java_com_fatecomputers_fcw_client_nativelib_FcwNative_releaseAll<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
    session_id: jlong,
    vm_id: JString<'l>,
) {
    guard((), || {
        let vm = jstr(&mut env, &vm_id);
        if let Some(s) = session(session_id) {
            s.release_all(&vm);
        }
        Ok(())
    })
}

#[no_mangle]
pub extern "system" fn Java_com_fatecomputers_fcw_client_nativelib_FcwNative_pollEvent<'l>(
    env: JNIEnv<'l>,
    _class: JClass<'l>,
    session_id: jlong,
) -> jstring {
    guard(std::ptr::null_mut(), || {
        let Some(ev) = session(session_id).and_then(|s| s.poll_event()) else {
            return Ok(std::ptr::null_mut());
        };
        Ok(env.new_string(ev)?.into_raw())
    })
}

#[no_mangle]
pub extern "system" fn Java_com_fatecomputers_fcw_client_nativelib_FcwNative_readAudio<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
    session_id: jlong,
    vm_id: JString<'l>,
    pcm: JShortArray<'l>,
) -> jint {
    guard(0, || {
        let vm = jstr(&mut env, &vm_id);
        let Some(s) = session(session_id) else { return Ok(0) };
        if pcm.is_null() {
            return Ok(0);
        }
        let len = env.get_array_length(&pcm)? as usize;
        let mut buf = vec![0i16; len];
        let n = s.read_audio(&vm, &mut buf);
        if n > 0 {
            env.set_short_array_region(&pcm, 0, &buf[..n])?;
        }
        Ok(n as jint)
    })
}
