# llmime-settings

macOS 13+ SwiftUI settings app for P6 local-LLM controls.

## Implemented P6 items

- Local LLM tab with mode selector: `N-gram` / `ハイブリッド` / `ローカルLLM`
- Model path display + auto-detect button (calls `llmime_config_scan_models_json`)
- Default model download button (calls `llmime_config_download_default_model`)
- Custom GGUF registration using file picker
- Estimated RAM display for selected scanned model
- Ollama endpoint settings (default `http://localhost:11434`)
- Startup load and save to `config.toml` via `llmime_config_load_json` + `llmime_config_save_settings`

## Build

```bash
cd apps/llmime-settings
swift run
```

Note: FFI symbols are provided by `llmime-core` and should be linked by the host integration step.
