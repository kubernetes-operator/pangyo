//! 기기 정보·파일 저장 (2026-11-13 사용 의견 4차)
//!
//! - 기기 이름: Android는 설정의 '기기 이름'(Settings.Global device_name, 없으면 제조사+모델), 데스크톱은 호스트 이름
//! - 기기 고유값: 다시 설치해도 같은 장비로 알아보기 위한 값. Android ID / PC 머신 ID를 **앱에서 한 번 해시**해서만 보냄
//!   (서버는 계정과 묶어 다시 해시해서 저장, 원래 값은 어디에도 남지 않음)
//! - 다운로드 저장: Android는 공용 '다운로드/MemoNest' (MediaStore), 데스크톱은 OS 다운로드 폴더
use sha2::{Digest, Sha256};

/// 원래 기기 값 → 보낼 값 (앱 이름을 섞어 다른 앱과 같은 값이 되지 않게)
fn hashed(raw: &str) -> Option<String> {
    let raw = raw.trim();
    if raw.len() < 8 {
        return None;
    }
    let digest = Sha256::digest(format!("memonest-device:{raw}").as_bytes());
    Some(digest.iter().take(16).map(|b| format!("{b:02x}")).collect())
}

/// 이 기기의 고유값(해시). 알 수 없으면 None → 서버는 새 장비로 등록
pub fn device_key() -> Option<String> {
    hashed(&raw_device_id()?)
}

#[cfg(target_os = "android")]
fn raw_device_id() -> Option<String> {
    android::secure_setting("android_id")
}

#[cfg(target_os = "linux")]
fn raw_device_id() -> Option<String> {
    ["/etc/machine-id", "/var/lib/dbus/machine-id"]
        .iter()
        .find_map(|p| std::fs::read_to_string(p).ok())
}

#[cfg(target_os = "windows")]
fn raw_device_id() -> Option<String> {
    // HKLM\SOFTWARE\Microsoft\Cryptography MachineGuid (윈도우 설치마다 고유)
    let out = std::process::Command::new("reg")
        .args([
            "query",
            r"HKLM\SOFTWARE\Microsoft\Cryptography",
            "/v",
            "MachineGuid",
        ])
        .output()
        .ok()?;
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .find(|l| l.contains("MachineGuid"))
        .and_then(|l| l.split_whitespace().last().map(str::to_owned))
}

#[cfg(target_os = "macos")]
fn raw_device_id() -> Option<String> {
    let out = std::process::Command::new("ioreg")
        .args(["-rd1", "-c", "IOPlatformExpertDevice"])
        .output()
        .ok()?;
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .find(|l| l.contains("IOPlatformUUID"))
        .and_then(|l| l.split('"').nth(3).map(str::to_owned))
}

#[cfg(not(any(
    target_os = "android",
    target_os = "linux",
    target_os = "windows",
    target_os = "macos"
)))]
fn raw_device_id() -> Option<String> {
    None
}

/// 화면 위쪽에서 비워 둘 높이(CSS px). Android 상태바, 다른 OS는 0
pub fn top_inset() -> f64 {
    #[cfg(target_os = "android")]
    {
        android::status_bar_height().unwrap_or(24.0)
    }
    #[cfg(not(target_os = "android"))]
    {
        0.0
    }
}

/// 화면 아래쪽에서 비워 둘 높이(CSS px). Android 내비게이션 막대, 다른 OS는 0
pub fn bottom_inset() -> f64 {
    #[cfg(target_os = "android")]
    {
        android::navigation_bar_inset().unwrap_or(0.0)
    }
    #[cfg(not(target_os = "android"))]
    {
        0.0
    }
}

/// 사람이 알아볼 기기 이름 (장비 등록 기본값)
pub fn device_name() -> Option<String> {
    #[cfg(target_os = "android")]
    {
        android::device_name()
    }
    #[cfg(not(target_os = "android"))]
    {
        let name = whoami::fallible::hostname().unwrap_or_default();
        let name = name.trim();
        let useless = name.is_empty()
            || name.eq_ignore_ascii_case("localhost")
            || name.eq_ignore_ascii_case("unknown");
        (!useless).then(|| name.chars().take(60).collect())
    }
}

#[cfg(target_os = "android")]
pub mod android {
    //! JNI로 Android API 호출 (Tauri가 이미 쓰는 jni·ndk-context, 앱 권한 추가 없음)
    use jni::{
        objects::{JObject, JString, JValue},
        JNIEnv, JavaVM,
    };

    #[allow(unsafe_code)] // 워크스페이스에서 유일한 예외 (Cargo.toml [workspace.lints] 참고)
    fn with_env<T>(
        f: impl FnOnce(&mut JNIEnv, &JObject) -> jni::errors::Result<T>,
    ) -> Result<T, String> {
        let ctx = ndk_context::android_context();
        // SAFETY: Tauri(Android 진입점)가 초기화한 프로세스의 JavaVM·Context 포인터
        let vm = unsafe { JavaVM::from_raw(ctx.vm().cast()) }.map_err(|e| e.to_string())?;
        let mut env = vm.attach_current_thread().map_err(|e| e.to_string())?;
        let context = unsafe { JObject::from_raw(ctx.context().cast()) };
        let result = f(&mut env, &context);
        if env.exception_check().unwrap_or(false) {
            let _ = env.exception_clear(); // Java 예외는 오류로 돌려주고 JVM 상태는 정리
        }
        result.map_err(|e| e.to_string())
    }

    fn content_resolver<'a>(
        env: &mut JNIEnv<'a>,
        context: &JObject,
    ) -> jni::errors::Result<JObject<'a>> {
        env.call_method(
            context,
            "getContentResolver",
            "()Landroid/content/ContentResolver;",
            &[],
        )?
        .l()
    }

    fn to_string(env: &mut JNIEnv, value: JObject) -> Option<String> {
        if value.is_null() {
            return None;
        }
        let s: String = env.get_string(&JString::from(value)).ok()?.into();
        let s = s.trim().to_owned();
        (!s.is_empty()).then_some(s)
    }

    fn setting(class: &str, key: &str) -> Option<String> {
        with_env(|env, context| {
            let resolver = content_resolver(env, context)?;
            let key = env.new_string(key)?;
            let value = env
                .call_static_method(
                    class,
                    "getString",
                    "(Landroid/content/ContentResolver;Ljava/lang/String;)Ljava/lang/String;",
                    &[JValue::Object(&resolver), JValue::Object(&key)],
                )?
                .l()?;
            Ok(to_string(env, value))
        })
        .ok()
        .flatten()
    }

    /// 상단 상태바 높이(CSS px = dp). 앱 화면이 상태바 밑까지 그려질 때 내려야 할 만큼 (2026-11-14 사용 의견)
    pub fn status_bar_height() -> Option<f64> {
        with_env(|env, context| {
            let res = env
                .call_method(
                    context,
                    "getResources",
                    "()Landroid/content/res/Resources;",
                    &[],
                )?
                .l()?;
            let name = env.new_string("status_bar_height")?;
            let kind = env.new_string("dimen")?;
            let pkg = env.new_string("android")?;
            let id = env
                .call_method(
                    &res,
                    "getIdentifier",
                    "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;)I",
                    &[
                        JValue::Object(&name),
                        JValue::Object(&kind),
                        JValue::Object(&pkg),
                    ],
                )?
                .i()?;
            if id == 0 {
                return Ok(None);
            }
            let px = env
                .call_method(&res, "getDimensionPixelSize", "(I)I", &[JValue::Int(id)])?
                .i()?;
            let metrics = env
                .call_method(
                    &res,
                    "getDisplayMetrics",
                    "()Landroid/util/DisplayMetrics;",
                    &[],
                )?
                .l()?;
            let density = env.get_field(&metrics, "density", "F")?.f()?;
            Ok((density > 0.0).then(|| f64::from(px) / f64::from(density)))
        })
        .ok()
        .flatten()
    }

    /// 하단 내비게이션 막대 높이(CSS px). 앱 화면이 막대 밑까지 그려질 때 아래 메뉴가 가려지지 않게 (2026-11-14 사용 의견).
    /// Android 11+의 실제 창 여백(WindowInsets)만 씀 — 제스처 내비게이션이면 작게, 가리지 않으면 0
    pub fn navigation_bar_inset() -> Option<f64> {
        with_env(|env, context| {
            let attempt = (|| -> jni::errors::Result<Option<f64>> {
                let window = env
                    .call_method(context, "getWindow", "()Landroid/view/Window;", &[])?
                    .l()?;
                let decor = env
                    .call_method(&window, "getDecorView", "()Landroid/view/View;", &[])?
                    .l()?;
                let insets = env
                    .call_method(
                        &decor,
                        "getRootWindowInsets",
                        "()Landroid/view/WindowInsets;",
                        &[],
                    )?
                    .l()?;
                if insets.is_null() {
                    return Ok(None);
                }
                let kind = env
                    .call_static_method(
                        "android/view/WindowInsets$Type",
                        "navigationBars",
                        "()I",
                        &[],
                    )?
                    .i()?;
                let bars = env
                    .call_method(
                        &insets,
                        "getInsets",
                        "(I)Landroid/graphics/Insets;",
                        &[JValue::Int(kind)],
                    )?
                    .l()?;
                let bottom = env.get_field(&bars, "bottom", "I")?.i()?;
                let res = env
                    .call_method(
                        context,
                        "getResources",
                        "()Landroid/content/res/Resources;",
                        &[],
                    )?
                    .l()?;
                let metrics = env
                    .call_method(
                        &res,
                        "getDisplayMetrics",
                        "()Landroid/util/DisplayMetrics;",
                        &[],
                    )?
                    .l()?;
                let density = env.get_field(&metrics, "density", "F")?.f()?;
                Ok((density > 0.0).then(|| f64::from(bottom) / f64::from(density)))
            })();
            match attempt {
                Ok(v) => Ok(v),
                Err(_) => {
                    let _ = env.exception_clear(); // Android 10 이하 등: 알 수 없음 → 0
                    Ok(None)
                }
            }
        })
        .ok()
        .flatten()
    }

    /// Settings.Secure (android_id 등)
    pub fn secure_setting(key: &str) -> Option<String> {
        setting("android/provider/Settings$Secure", key)
    }

    /// 설정의 '기기 이름', 없으면 "제조사 모델"
    pub fn device_name() -> Option<String> {
        if let Some(name) = setting("android/provider/Settings$Global", "device_name") {
            return Some(name.chars().take(60).collect());
        }
        with_env(|env, _| {
            let field = |env: &mut JNIEnv, name: &str| -> jni::errors::Result<Option<String>> {
                let v = env
                    .get_static_field("android/os/Build", name, "Ljava/lang/String;")?
                    .l()?;
                Ok(to_string(env, v))
            };
            let maker = field(env, "MANUFACTURER")?.unwrap_or_default();
            let model = field(env, "MODEL")?.unwrap_or_default();
            let name = if model.to_lowercase().starts_with(&maker.to_lowercase()) {
                model
            } else {
                format!("{maker} {model}")
            };
            Ok((!name.trim().is_empty()).then(|| name.trim().chars().take(60).collect()))
        })
        .ok()
        .flatten()
    }

    /// 공용 '다운로드/MemoNest/<이름>'에 저장 (MediaStore, Android 10+). 저장한 위치(사람이 읽는 경로)를 돌려줌
    pub fn save_to_downloads(file_name: &str, mime: &str, data: &[u8]) -> Result<String, String> {
        with_env(|env, context| {
            let resolver = content_resolver(env, context)?;
            let values = env.new_object("android/content/ContentValues", "()V", &[])?;
            for (key, value) in [
                ("_display_name", file_name),
                ("mime_type", mime),
                ("relative_path", "Download/MemoNest"),
            ] {
                let k = env.new_string(key)?;
                let v = env.new_string(value)?;
                env.call_method(
                    &values,
                    "put",
                    "(Ljava/lang/String;Ljava/lang/String;)V",
                    &[JValue::Object(&k), JValue::Object(&v)],
                )?;
            }
            let collection = env
                .get_static_field(
                    "android/provider/MediaStore$Downloads",
                    "EXTERNAL_CONTENT_URI",
                    "Landroid/net/Uri;",
                )?
                .l()?;
            let uri = env
                .call_method(
                    &resolver,
                    "insert",
                    "(Landroid/net/Uri;Landroid/content/ContentValues;)Landroid/net/Uri;",
                    &[JValue::Object(&collection), JValue::Object(&values)],
                )?
                .l()?;
            if uri.is_null() {
                return Err(jni::errors::Error::NullPtr("MediaStore insert"));
            }
            let stream = env
                .call_method(
                    &resolver,
                    "openOutputStream",
                    "(Landroid/net/Uri;)Ljava/io/OutputStream;",
                    &[JValue::Object(&uri)],
                )?
                .l()?;
            let bytes = env.byte_array_from_slice(data)?;
            env.call_method(&stream, "write", "([B)V", &[JValue::Object(&bytes)])?;
            env.call_method(&stream, "close", "()V", &[])?;
            Ok(format!("Download/MemoNest/{file_name}"))
        })
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn device_key_is_hashed_and_stable() {
        let a = super::hashed("0123456789abcdef").unwrap();
        assert_eq!(a, super::hashed(" 0123456789abcdef\n").unwrap());
        assert_eq!(a.len(), 32);
        assert!(!a.contains("0123456789abcdef"));
        assert!(super::hashed("short").is_none());
    }
}
