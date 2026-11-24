## 1. Манифест и окружение сборки

- [x] 1.1 `Cargo.toml`: `[package] name = "thepaper"`, таблицу `[[bin]]` удалить (имя бинаря по умолчанию совпадает с именем пакета); проверка — `direnv exec . cargo metadata --no-deps --format-version 1 | jq -r '.packages[].name, .packages[].targets[].name'` печатает только `thepaper`
- [x] 1.2 `flake.nix`: описание → `"thepaper: zero-copy VA-API/Vulkan live video wallpaper for niri"`; проверка — `rg -n 'description = ' flake.nix` показывает новое имя и не содержит `video-wallpaper`
- [x] 1.3 Сборка по «Среда» (`direnv exec . chrt -i 0 nice -n 19 ionice -c 3 env CARGO_BUILD_JOBS=10 cargo build -j 10`): `Cargo.lock` перегенерирован, артефакт `target/debug/thepaper` существует; проверка — `test -x target/debug/thepaper` и `rg -n '^name = "thepaper"$' Cargo.lock`

## 2. Код: пользовательские имена

- [x] 2.1 `src/paths.rs`: лист каталога конфига и кеша → `thepaper` (структура `config.toml` и каталога кеша не меняется); проверка — `rg -n 'video-wallpaper' src/paths.rs` пусто, `rg -n 'thepaper' src/paths.rs` находит оба пути
- [x] 2.2 `src/config.rs`: `default_app_id_prefix` → `thepaper`; проверка — `rg -n 'thepaper' src/config.rs` находит префикс, `rg -n 'video-wallpaper' src/config.rs` пусто
- [x] 2.3 `src/instance.rs`: лок `thepaper.lock` и текст ошибки о втором инстансе → `thepaper`; проверка — `rg -n 'thepaper' src/instance.rs` находит оба места
- [x] 2.4 `src/decoder.rs`: `VIDEO_WALLPAPER_DRM_RENDER_NODE` → `THEPAPER_DRM_RENDER_NODE`; проверка — `rg -n 'THEPAPER_DRM_RENDER_NODE' src/decoder.rs`
- [x] 2.5 `src/main.rs`: `VIDEO_WALLPAPER_LOG` → `THEPAPER_LOG`, стартовое сообщение → `starting thepaper`; проверка — `rg -n 'THEPAPER_LOG|starting thepaper' src/main.rs`
- [x] 2.6 `src/app.rs`, `src/render.rs`: метки GPU-объектов → `thepaper`, `thepaper shader`, `thepaper layout`; проверка — `rg -n 'video-wallpaper' src/` пусто

## 3. Документация

- [x] 3.1 `README.md`: заголовок `# thepaper`, переменные окружения, пути конфига/кеша/лока, пример конфига (`app_id_prefix = "thepaper"`), niri `match namespace="^thepaper-blur-"` и `spawn-at-startup "thepaper"`, пример логов (`THEPAPER_LOG='thepaper=debug,...' thepaper`); проверка — `rg -n 'video-wallpaper|VIDEO_WALLPAPER|video_wallpaper' README.md` находит совпадения только внутри раздела «Migration from video-wallpaper»
- [x] 3.2 `README.md`: блок миграции — `mv ~/.config/video-wallpaper ~/.config/thepaper`, `rm -rf ~/.cache/video-wallpaper`, правка правила niri и автозапуска; проверка — блок присутствует и называет все три действия

## 4. OpenSpec: домен спек

- [x] 4.1 `git mv openspec/specs/video-wallpaper openspec/specs/thepaper`, в каждом `spec.md` заголовок `# <capability> Specification` → `# thepaper/<capability> Specification`; проверка — `find openspec/specs -maxdepth 2 -type d` показывает только `openspec/specs/thepaper`, `rg -n '^# video-wallpaper/' openspec/specs` пусто
- [x] 4.2 Если изменение `move-video-wallpaper-to-src` ещё не заархивировано — `git mv openspec/changes/move-video-wallpaper-to-src/specs/video-wallpaper openspec/changes/move-video-wallpaper-to-src/specs/thepaper` (тексты proposal/design/tasks не трогаются); проверка — путь `.../specs/thepaper/playback/spec.md` существует либо каталог изменения уже в `openspec/changes/archive/`
- [x] 4.3 `openspec validate rename-project-to-thepaper` и `openspec list --json`; проверка — валидация без ошибок, единственное активное изменение — `rename-project-to-thepaper` (`--strict` даёт только предупреждения RFC 2119 на русские `ДОЛЖНА`)

## 5. Проверка

- [x] 5.1 Старое имя не осталось в рабочем дереве: `rg -n --hidden -g '!target/**' -g '!.git/**' -g '!.direnv/**' -g '!openspec/changes/archive/**' -g '!openspec/changes/rename-project-to-thepaper/**' -g '!README.md' 'video-wallpaper|VIDEO_WALLPAPER|video_wallpaper'` не даёт совпадений; в README старое имя допустимо только в разделе миграции (проверено в 3.1)
- [x] 5.2 `direnv exec . chrt -i 0 nice -n 19 ionice -c 3 env CARGO_BUILD_JOBS=10 cargo clippy --all-targets -j 10` и `cargo test -j 10` (с `RUST_TEST_THREADS=10`) — чисто
- [x] 5.3 Контейнерный прогон по «Среда» (headless Xwayland, ввод xdotool, скриншот): `thepaper` играет обои, namespace-ы в `wayland-info`/логе начинаются с `thepaper-`, конфиг читается из нового пути; без hardware GPU прогон не выполняется, ограничение фиксируется в итоговом сообщении
