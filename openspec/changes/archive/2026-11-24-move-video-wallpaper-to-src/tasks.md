## 1. Перенос крейта в корень

- [x] 1.1 `git mv crates/video-wallpaper/src src` (файлы и `shader.wgsl`); убедиться, что `git status` показывает rename без правок содержимого
- [x] 1.2 Корневой `Cargo.toml`: удалить секцию `[workspace]`, в `[package]` вписать `edition = "2021"`, `license = "MIT"`, `rust-version = "1.89"`; перенести остальные поля крейта из `crates/video-wallpaper/Cargo.toml`; удалить `crates/`
- [x] 1.3 Сборка по «Среда» (`direnv exec . chrt -i 0 nice -n 19 ionice -c 3 env CARGO_BUILD_JOBS=10 cargo build -j 10`): `Cargo.lock` перегенерирован, бинарь `video-wallpaper` собирается; путь крейта в `Cargo.lock` — корневой

## 2. Ревью-исправления: без паники

- [x] 2.1 Все `Mutex::lock().unwrap()` (decoder/control, mailbox, priority) → `lock().unwrap_or_else(std::sync::PoisonError::into_inner)`; grep по `src/` не находит `unwrap()`/`expect(` вне `#[cfg(test)]`
- [x] 2.2 `Decoder::open`: `CString::new(render_node())` валидируется через `with_context`, без `unwrap`; `open_inner` — обычная приватная `fn` без `unsafe fn` (unsafe-блоки точечные)
- [x] 2.3 `Decoder::next_frame`: sentinel `"__eof__"` заменён на `enum ReceiveOutcome { Frame(AvFrame), NeedPacket, Eof }` — результат `receive_into_pending`; `match` вместо сравнения строк ошибки; поля `pending`/`mapped` из `Decoder` исчезли, маппинг локален
- [x] 2.4 `App::configure_entry`: guard `let Some(frame) = ... else { return; }` наверху, `frame.unwrap()` удалён; `Renderer::ensure_blur`: `ensure_blur_targets` возвращает `&BlurTargets`, `.unwrap()` удалён
- [x] 2.5 `niri::spawn` и `gamemode::spawn` возвращают `Result<JoinHandle<()>>`; `main.rs` прокидывает ошибку в диагностический выход; падение уже запущенного потока по-прежнему переводит обои в базовый режим
- [x] 2.6 `Renderer::import`: неиспользуемый параметр `queue` удалён; счётчики-scaffolding (`impl Drop for VideoFrame`, `HITS`) удалены

## 3. Ревью-исправления: поток данных

- [x] 3.1 Один путь на потребителя: `Control` — только контракт декодера (`game_active`, `shutdown`); поле `overview_open` и `set_overview_open` удалены, `niri.rs` пишет только в канал; `App` реагирует только на события calloop-канала; `sync_control_state` и вызов в `dispatch_loop` удалены
- [x] 3.2 `Renderer` — единственный источник истины о блюре: `ensure_blur` возвращает `()` с guard-ом по `blur_valid_seq`; предпроверки `blur_valid(seq)` в `configure_entry`/`redraw` удалены; `blur_valid` остаётся для проверки показа без пересчёта
- [x] 3.3 `niri::connect_and_stream` возвращает `enum NiriOutcome { Unavailable, StreamEnded }`; внешний цикл в `spawn` матчит исход
- [x] 3.4 `frame_key` — ключ только по inode dmabuf-fd (вырожденная компонента `planes.len()` удалена), чтение через `planes.first()`

## 4. Ревью-исправления: файлы, сущности, имена

- [x] 4.1 Удалить пустой `compositor.rs` и `mod compositor;` из `main.rs`
- [x] 4.2 Разрезать `decoder.rs`: `decoder.rs` (FFI-обёртки, `Decoder`, `render_node`), `frame.rs` (`DmaPlane`/`VideoFrame`/`ColorInfo`/`PlaneFormat`), `control.rs` (`Control`, `Mailbox`, `FrameMsg`), `worker.rs` (`Worker`, `spawn_thread`)
- [x] 4.3 Разрезать `app.rs`: `app.rs` (`App` + trait-impls), `surface.rs` (`SurfaceEntry`, `OutputPair`, `DrawMode`, `surface_from_wl`), `sources.rs` (`install_sources`, `handle_niri_event`, `handle_game_event`, `dispatch_loop`); разрезать `transcode.rs`: `codec.rs` (выбор цели) + `transcode.rs` (исполнение и кеш)
- [x] 4.4 Вынести из `main.rs`/`config.rs`: `instance.rs` (`InstanceLock`, `acquire_instance_lock`), `paths.rs` (`default_config_path`, `cache_dir`); `main.rs` — только `mod` + `pub use` (алфавитно) и точка входа
- [x] 4.5 Сущности: поля `App` и `Renderer::blur_radius` — приватные; `retired` → `RetiredFrame { frame, generation }`; размеры `(u32, u32)` в сигнатурах → value-тип `Size { width, height }`
- [x] 4.6 Дубль: три тела render-pass (`draw_live`/`ensure_blur`×2/`draw_blur`) → один хелпер `draw_triangle`; `configure_entry(blur: bool)` → параметр `DrawMode`; `set_game_active(bool)` → enum `GameScheduling`; clamp масштаба — один раз в конструкторе `OutputPair`; `keys: Vec<ObjectId>` в `redraw` → деструктурирующий borrow
- [x] 4.7 Прочее: `Control::set_overview_open` — удалён вместе с полем (см. 3.1); `map_pending` — итераторы по слоям/плоскостям вместо индексных циклов, сложное tracing-выражение в `open_inner` → именованный `let decoder_name`; `free_transient_state` → `trim_heap`
- [x] 4.8 Удалить комментарии и doc-комментарии; где смысл не читается из имени — переименовать (геттеры без `get_`, предикаты — глаголы)
- [x] 4.9 Прогнать алгоритм «Ревью» из AGENTS.md по каждому изменённому/новому файлу; находки правятся, ответы шагов зафиксированы в итоговом сообщении

## 5. Проверка

- [x] 5.1 `cargo clippy --all-targets -j 10` и `cargo test -j 10` (низкий приоритет, RUST_TEST_THREADS=10) — чисто
- [x] 5.2 Контейнерный прогон по «Среда» (headless Xwayland, ввод xdotool, скриншот): обои играют, овервью показывает блюр; без hardware GPU — прогон не выполняется, ограничение фиксируется в итоговом сообщении
