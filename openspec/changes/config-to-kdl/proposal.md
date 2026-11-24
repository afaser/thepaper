## Why

Конфигурация `thepaper` — единственный пользовательский файл проекта и сейчас она в TOML, тогда как весь остальной конфиг рабочего окружения (niri, wayland-компоновщик) ведётся в KDL. Пользователь просит один язык конфигурации: `~/.config/thepaper/config.kdl` читается и правится теми же привычками, что и `~/.config/niri/config.kdl`.

## What Changes

- **BREAKING**: путь конфигурации по умолчанию — `$XDG_CONFIG_HOME/thepaper/config.kdl` (фолбэк `~/.config/thepaper/config.kdl`); `config.toml` больше не читается, фолбэка на старый формат нет.
- **BREAKING**: содержимое файла — KDL-документ: по одному узлу на опцию, значение — аргумент узла:
  ```kdl
  video_path "/home/user/Videos/wallpaper.mp4"
  blur_radius 12
  app_id_prefix "thepaper"
  transcode_codec "hevc"
  ```
- Имена опций, дефолты (`blur_radius` = 12, `app_id_prefix` = `thepaper`), диапазон `1..=64`, обязательность `video_path` и проверка существования файла не меняются.
- Ошибки конфигурации называют путь файла и проблемный узел: неизвестный узел, повтор узла, неверный тип аргумента, отсутствие `video_path`, недопустимый `blur_radius`, несуществующее видео.
- Зависимости: `toml` и прямой `serde` (derive для `Config`) удаляются, добавляется `kdl` из crates.io; `serde` остаётся транзитивной зависимостью `zbus`/`niri-ipc`.
- README: пример конфигурации переписывается на KDL, путь — `config.kdl`, добавляется миграционный блок «переименуй файл и перепиши опции».

## Capabilities

### New Capabilities

- `thepaper/configuration`: формат файла конфигурации (KDL), состав и имена опций, дефолты, валидация и диагностика ошибок.

### Modified Capabilities

- `thepaper/identity`: путь конфигурации по умолчанию меняется с `config.toml` на `config.kdl`; требование к явному `--config` сохраняется.

## Impact

- Код: `src/config.rs` (парсинг KDL вместо `toml::from_str`), новый файл-концепт для разобранного KDL-документа, `src/paths.rs` (лист `config.kdl`), `src/main.rs` (без изменений логики), `Cargo.toml`, `Cargo.lock`.
- Спеки: новая `openspec/specs/thepaper/configuration/spec.md`, дельта `openspec/specs/thepaper/identity/spec.md`.
- Пользовательская среда: существующий `config.toml` переписывается вручную; приложение при отсутствии `config.kdl` завершается ошибкой чтения файла.
- API и поведение воспроизведения, поверхностей, овервью и game-mode не меняются.
