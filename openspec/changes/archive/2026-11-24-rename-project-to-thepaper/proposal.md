## Why

Проект называется `thepaper` (так назван каталог репозитория), но исполняемый файл, пути XDG, namespace-ы поверхностей, переменные окружения и документация используют самопридуманное `video-wallpaper`. Пользователь видит чужое имя везде: в `cargo build`, в конфиге, в правилах niri, в логах.

## What Changes

- **BREAKING**: исполняемый файл и имя пакета — `video-wallpaper` → `thepaper` (`[package] name`, `[[bin]] name`, `Cargo.lock` перегенерирован).
- **BREAKING**: пути по умолчанию — конфиг `$XDG_CONFIG_HOME/thepaper/config.toml` (фолбэк `~/.config/thepaper/config.toml`), кеш транскода `$XDG_CACHE_HOME/thepaper/`, лок единственного инстанса `$XDG_RUNTIME_DIR/thepaper.lock`. Старые пути не читаются и не создаются: чистый переход без фолбэков.
- **BREAKING**: переменные окружения — `THEPAPER_LOG` (фильтр логов) и `THEPAPER_DRM_RENDER_NODE` (узел DRM render); имена `VIDEO_WALLPAPER_*` больше не действуют.
- **BREAKING**: префикс app-id по умолчанию — `thepaper`; namespace-ы поверхностей `<prefix>-live-<output>` и `<prefix>-blur-<output>`, то есть `thepaper-live-<output>` и `thepaper-blur-<output>`. Правила композитора у пользователя обновляются вместе с приложением.
- Внутренние идентификаторы: цель логов и стартовое сообщение, метки GPU-объектов, текст ошибки о втором инстансе.
- Документация и окружение: README (заголовок, примеры конфига, niri-правила, запуск, логи), описание `flake.nix`.
- OpenSpec: домен capability `openspec/specs/video-wallpaper/` переименовывается в `openspec/specs/thepaper/` вместе с заголовками спек; добавляется capability `thepaper/identity`, фиксирующая пользовательские имена. Дельта ещё не заархивированного изменения `move-video-wallpaper-to-src` переносится в `thepaper/playback`, чтобы будущий архив не воскресил старый домен.
- Историю не переписываем: заархивированное изменение `add-video-wallpaper` и имя незавершённого `move-video-wallpaper-to-src` остаются как есть.

## Capabilities

### New Capabilities

- `thepaper/identity`: имя исполняемого файла, пути конфига/кеша/лока, переменные окружения и префикс идентификаторов поверхностей — всё, по чему пользователь и композитор находят приложение.

### Modified Capabilities

Нет: поведение воспроизведения, поверхностей, овервью и game-mode не меняется. Переименование домена спек — структурная операция (перенос каталога), новых требований в существующие capability не добавляет.

## Impact

- Код: `Cargo.toml`, `Cargo.lock`, `flake.nix`, `README.md`, `src/paths.rs`, `src/config.rs`, `src/instance.rs`, `src/decoder.rs`, `src/main.rs`, `src/app.rs`, `src/render.rs`.
- Спеки: `openspec/specs/video-wallpaper/*` → `openspec/specs/thepaper/*`; новая `openspec/specs/thepaper/identity/spec.md` появляется при архиве.
- Пользовательская среда: конфиг переезжает вручную (`~/.config/video-wallpaper/` → `~/.config/thepaper/`), кеш транскода пересоздаётся, правила niri переписываются на `^thepaper-blur-`, автозапуск — `spawn-at-startup "thepaper"`.
- Зависимости и API не меняются.
