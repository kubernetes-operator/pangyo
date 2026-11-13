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
