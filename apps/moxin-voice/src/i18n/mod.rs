//! Internationalization (i18n) module for Moxin Voice
//!
//! Supports English and Chinese localization

use std::collections::HashMap;
use std::sync::Mutex;
use once_cell::sync::Lazy;

/// Supported locales
pub const SUPPORTED_LOCALES: &[&str] = &["en", "zh"];

/// Default locale
pub const DEFAULT_LOCALE: &str = "zh";

/// Current locale
static CURRENT_LOCALE: Lazy<Mutex<String>> = Lazy::new(|| Mutex::new(DEFAULT_LOCALE.to_string()));

/// Translation map: locale -> key -> value
static TRANSLATIONS: Lazy<Mutex<HashMap<String, HashMap<String, String>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

/// Initialize translations
pub fn init_translations() {
    let mut translations = TRANSLATIONS.lock().unwrap();

    // English translations
    let mut en = HashMap::new();
    en.insert("app.title".to_string(), "Moxin Voice".to_string());
    en.insert("nav.text_to_speech".to_string(), "Text to Speech".to_string());
    en.insert("nav.voice_library".to_string(), "Voice Library".to_string());
    en.insert("nav.voice_clone".to_string(), "Voice Clone".to_string());
    en.insert("nav.settings".to_string(), "Settings".to_string());
    en.insert("voice.select".to_string(), "Select Voice".to_string());
    en.insert("voice.search".to_string(), "Search voices...".to_string());
    en.insert("voice.filter.all".to_string(), "All".to_string());
    en.insert("voice.filter.male".to_string(), "Male".to_string());
    en.insert("voice.filter.female".to_string(), "Female".to_string());
    en.insert("voice.filter.character".to_string(), "Character".to_string());
    en.insert("voice.filter.custom".to_string(), "My".to_string());
    en.insert("voice.language.all".to_string(), "All".to_string());
    en.insert("voice.language.chinese".to_string(), "Chinese".to_string());
    en.insert("voice.language.english".to_string(), "English".to_string());
    en.insert("voice.clone".to_string(), "Clone Voice".to_string());
    en.insert("voice.delete".to_string(), "Delete".to_string());
    en.insert("voice.preview".to_string(), "Preview".to_string());
    en.insert("tts.generate".to_string(), "Generate".to_string());
    en.insert("tts.generating".to_string(), "Generating...".to_string());
    en.insert("tts.play".to_string(), "Play".to_string());
    en.insert("tts.pause".to_string(), "Pause".to_string());
    en.insert("tts.stop".to_string(), "Stop".to_string());
    en.insert("tts.export".to_string(), "Export".to_string());
    en.insert("tts.input_placeholder".to_string(), "Enter text to convert to speech...".to_string());
    en.insert("tts.speed".to_string(), "Speed".to_string());
    en.insert("tts.pitch".to_string(), "Pitch".to_string());
    en.insert("tts.volume".to_string(), "Volume".to_string());
    en.insert("tts.no_result".to_string(), "No result yet".to_string());
    en.insert("clone.express_mode".to_string(), "Express Mode".to_string());
    en.insert("clone.express_desc".to_string(), "Clone voice with 5-10 seconds of audio".to_string());
    en.insert("clone.pro_mode".to_string(), "Pro Mode".to_string());
    en.insert("clone.pro_desc".to_string(), "High-quality voice with 3-10 minutes training".to_string());
    en.insert("clone.record_audio".to_string(), "Record Audio".to_string());
    en.insert("clone.upload_audio".to_string(), "Upload Audio".to_string());
    en.insert("clone.start_recording".to_string(), "Start Recording".to_string());
    en.insert("clone.stop_recording".to_string(), "Stop Recording".to_string());
    en.insert("clone.training".to_string(), "Training".to_string());
    en.insert("clone.train".to_string(), "Train".to_string());
    en.insert("clone.training_progress".to_string(), "Training Progress".to_string());
    en.insert("settings.title".to_string(), "Settings".to_string());
    en.insert("settings.voice_settings".to_string(), "Voice Settings".to_string());
    en.insert("settings.default_speed".to_string(), "Default Speed".to_string());
    en.insert("settings.default_pitch".to_string(), "Default Pitch".to_string());
    en.insert("settings.default_volume".to_string(), "Default Volume".to_string());
    en.insert("settings.output_format".to_string(), "Output Format".to_string());
    en.insert("settings.general".to_string(), "General".to_string());
    en.insert("settings.language".to_string(), "Language".to_string());
    en.insert("settings.theme".to_string(), "Theme".to_string());
    en.insert("settings.theme_light".to_string(), "Light".to_string());
    en.insert("settings.theme_dark".to_string(), "Dark".to_string());
    en.insert("settings.data_path".to_string(), "Data Path".to_string());
    en.insert("settings.about".to_string(), "About".to_string());
    en.insert("settings.version".to_string(), "Version".to_string());
    en.insert("settings.check_update".to_string(), "Check for Updates".to_string());
    en.insert("common.confirm".to_string(), "Confirm".to_string());
    en.insert("common.cancel".to_string(), "Cancel".to_string());
    en.insert("common.save".to_string(), "Save".to_string());
    en.insert("common.delete".to_string(), "Delete".to_string());
    en.insert("common.close".to_string(), "Close".to_string());
    en.insert("common.back".to_string(), "Back".to_string());
    en.insert("common.loading".to_string(), "Loading...".to_string());
    en.insert("common.error".to_string(), "Error".to_string());
    en.insert("common.success".to_string(), "Success".to_string());
    en.insert("status.ready".to_string(), "Ready".to_string());
    en.insert("status.dora_running".to_string(), "Dora Running".to_string());
    en.insert("status.dora_stopped".to_string(), "Dora Stopped".to_string());
    en.insert("error.dora_not_running".to_string(), "Please start Dora first".to_string());
    en.insert("error.no_voice_selected".to_string(), "Please select a voice".to_string());
    en.insert("error.no_text_input".to_string(), "Please enter text".to_string());

    // Chinese translations
    let mut zh = HashMap::new();
    zh.insert("app.title".to_string(), "魔音TTS".to_string());
    zh.insert("nav.text_to_speech".to_string(), "语音合成".to_string());
    zh.insert("nav.voice_library".to_string(), "音色库".to_string());
    zh.insert("nav.voice_clone".to_string(), "声音克隆".to_string());
    zh.insert("nav.settings".to_string(), "设置".to_string());
    zh.insert("voice.select".to_string(), "选择音色".to_string());
    zh.insert("voice.search".to_string(), "搜索音色...".to_string());
    zh.insert("voice.filter.all".to_string(), "全部".to_string());
    zh.insert("voice.filter.male".to_string(), "男声".to_string());
    zh.insert("voice.filter.female".to_string(), "女声".to_string());
    zh.insert("voice.filter.character".to_string(), "角色".to_string());
    zh.insert("voice.filter.custom".to_string(), "我的".to_string());
    zh.insert("voice.language.all".to_string(), "全部".to_string());
    zh.insert("voice.language.chinese".to_string(), "中文".to_string());
    zh.insert("voice.language.english".to_string(), "英文".to_string());
    zh.insert("voice.clone".to_string(), "克隆音色".to_string());
    zh.insert("voice.delete".to_string(), "删除".to_string());
    zh.insert("voice.preview".to_string(), "预览".to_string());
    zh.insert("tts.generate".to_string(), "生成".to_string());
    zh.insert("tts.generating".to_string(), "生成中...".to_string());
    zh.insert("tts.play".to_string(), "播放".to_string());
    zh.insert("tts.pause".to_string(), "暂停".to_string());
    zh.insert("tts.stop".to_string(), "停止".to_string());
    zh.insert("tts.export".to_string(), "导出".to_string());
    zh.insert("tts.input_placeholder".to_string(), "请输入要转换的文字...".to_string());
    zh.insert("tts.speed".to_string(), "语速".to_string());
    zh.insert("tts.pitch".to_string(), "音调".to_string());
    zh.insert("tts.volume".to_string(), "音量".to_string());
    zh.insert("tts.no_result".to_string(), "暂无结果".to_string());
    zh.insert("clone.express_mode".to_string(), "快速模式".to_string());
    zh.insert("clone.express_desc".to_string(), "5-10秒音频克隆声音".to_string());
    zh.insert("clone.pro_mode".to_string(), "专业模式".to_string());
    zh.insert("clone.pro_desc".to_string(), "3-10分钟训练高质量克隆".to_string());
    zh.insert("clone.record_audio".to_string(), "录制音频".to_string());
    zh.insert("clone.upload_audio".to_string(), "上传音频".to_string());
    zh.insert("clone.start_recording".to_string(), "开始录制".to_string());
    zh.insert("clone.stop_recording".to_string(), "停止录制".to_string());
    zh.insert("clone.training".to_string(), "训练中".to_string());
    zh.insert("clone.train".to_string(), "训练".to_string());
    zh.insert("clone.training_progress".to_string(), "训练进度".to_string());
    zh.insert("settings.title".to_string(), "设置".to_string());
    zh.insert("settings.voice_settings".to_string(), "语音合成设置".to_string());
    zh.insert("settings.default_speed".to_string(), "默认语速".to_string());
    zh.insert("settings.default_pitch".to_string(), "默认音调".to_string());
    zh.insert("settings.default_volume".to_string(), "默认音量".to_string());
    zh.insert("settings.output_format".to_string(), "输出格式".to_string());
    zh.insert("settings.general".to_string(), "通用设置".to_string());
    zh.insert("settings.language".to_string(), "语言".to_string());
    zh.insert("settings.theme".to_string(), "主题".to_string());
    zh.insert("settings.theme_light".to_string(), "浅色".to_string());
    zh.insert("settings.theme_dark".to_string(), "深色".to_string());
    zh.insert("settings.data_path".to_string(), "数据路径".to_string());
    zh.insert("settings.about".to_string(), "关于".to_string());
    zh.insert("settings.version".to_string(), "版本".to_string());
    zh.insert("settings.check_update".to_string(), "检查更新".to_string());
    zh.insert("common.confirm".to_string(), "确认".to_string());
    zh.insert("common.cancel".to_string(), "取消".to_string());
    zh.insert("common.save".to_string(), "保存".to_string());
    zh.insert("common.delete".to_string(), "删除".to_string());
    zh.insert("common.close".to_string(), "关闭".to_string());
    zh.insert("common.back".to_string(), "返回".to_string());
    zh.insert("common.loading".to_string(), "加载中...".to_string());
    zh.insert("common.error".to_string(), "错误".to_string());
    zh.insert("common.success".to_string(), "成功".to_string());
    zh.insert("status.ready".to_string(), "就绪".to_string());
    zh.insert("status.dora_running".to_string(), "Dora 运行中".to_string());
    zh.insert("status.dora_stopped".to_string(), "Dora 已停止".to_string());
    zh.insert("error.dora_not_running".to_string(), "请先启动 Dora".to_string());
    zh.insert("error.no_voice_selected".to_string(), "请选择音色".to_string());
    zh.insert("error.no_text_input".to_string(), "请输入文字".to_string());

    translations.insert("en".to_string(), en);
    translations.insert("zh".to_string(), zh);
}

/// Get current locale
pub fn get_locale() -> String {
    CURRENT_LOCALE.lock().unwrap().clone()
}

/// Set current locale
pub fn set_locale(locale: &str) -> Result<(), String> {
    if !SUPPORTED_LOCALES.contains(&locale) {
        return Err(format!("Unsupported locale: {}", locale));
    }
    *CURRENT_LOCALE.lock().unwrap() = locale.to_string();
    Ok(())
}

/// Translate a key using current locale
/// Supports dot notation: "nav.text_to_speech"
pub fn t(key: &str) -> String {
    let locale = get_locale();
    t_with_locale(key, &locale)
}

/// Translate a key with specific locale
pub fn t_with_locale(key: &str, locale: &str) -> String {
    let translations = TRANSLATIONS.lock().unwrap();

    if let Some(locale_map) = translations.get(locale) {
        if let Some(value) = locale_map.get(key) {
            return value.clone();
        }
    }

    // Fallback to English
    if locale != "en" {
        if let Some(en_map) = translations.get("en") {
            if let Some(value) = en_map.get(key) {
                return value.clone();
            }
        }
    }

    // Return key if not found
    key.to_string()
}
