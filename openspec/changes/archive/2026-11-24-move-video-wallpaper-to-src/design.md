## Context

Один крейт-бинарь `video-wallpaper` живёт в `crates/video-wallpaper/`, корневой `Cargo.toml` — `[workspace]` с единственным member и таблицей `[workspace.package]`, от которой крейт наследует `edition`/`license`/`rust-version`. Вне `target/` на путь крейта ссылается только корневой `Cargo.toml`; `flake.nix`, `.envrc` и README пути не упоминают. Открывавший изменения архив (`add-video-wallpaper`) уже заархивирован, спеки описывают поведение, а не раскладку файлов.

Ревью исходников по AGENTS.md («Ревью», шаги 1–6) нашло нарушения, перечисленные в proposal.md.

## Goals / Non-Goals

**Goals:**

- Плоский корневой `src/` — каталог файлов-концептов, крейт без обёртки workspace.
- Ноль паники в рабочем коде; отказ старта — `Result` → диагностический выход, отказ потока-наблюдателя — продолжение в базовом режиме.
- Каждый файл — одна ответственность; индекс `main.rs` без логики.
- Смысл — в именах, не в комментариях.

**Non-Goals:**

- Изменение поведения: конфиг, CLI, namespace-и, транскод-кеш, блюр, game-mode — как в спеках.
- Новые зависимости; рефакторинг логики за пределами перечисленных исправлений.

## Decisions

**Раскладка.** `git mv crates/video-wallpaper/src/* src/` (сохраняет историю), `shader.wgsl` едет вместе с исходниками — `include_str!("shader.wgsl")` в `render.rs` относительный, пути не ломаются. Корневой `Cargo.toml`: убрать секцию `[workspace]`, в `[package]` вписать `edition = "2021"`, `license = "MIT"`, `rust-version = "1.89"` вместо `*.workspace = true`. `crates/` удалить. `Cargo.lock` перечитается (`cargo update --workspace` не нужен — просто сборка перегенерирует пути). Альтернатива «оставить workspace с одной нодой» отклонена:AGENTS.md требует плоский каталог без лишней прослойки.

**Разделение файлов** (шаг 1 «Ревью», результаты в review.md; новые файлы — плоско в `src/`, реэкспорт через `pub use` в `main.rs`, алфавитный порядок):

- `decoder.rs` (758 строк, 5 тем) → `decoder.rs` (FFI-обёртки `AvFrame`/`FormatCtx`/`CodecCtx`/`HwDevice`/`Packet` + `Decoder` + `render_node`), `frame.rs` (`DmaPlane`, `VideoFrame`, `ColorInfo`, `PlaneFormat` — кадр для GPU-импорта), `control.rs` (`Control`, `Mailbox`, `FrameMsg` — контракт декодер↔event-loop), `worker.rs` (`Worker`, `spawn_thread` — жизненный цикл потока и handoff). `render_node()` остаётся в `decoder.rs` (вызовы — `Decoder::open` и `transcode::prepare` через полный путь крейта).
- `app.rs` (879 строк, 3 темы) → `app.rs` (`App` + trait-impl Wayland-хендлеров), `surface.rs` (`SurfaceEntry`, `OutputPair`, `DrawMode`, а также `surface_from_wl` — свободная функция рядом с единственным вызовом в `SurfaceEntry::new`), `sources.rs` (`install_sources`, `handle_niri_event`, `handle_game_event`, `dispatch_loop` — проводка calloop). Альтернатива «дробить `App` на подсистему-каталог» отклонена: вложенность оправдана только между слоями/подсистемами, здесь один слой.
- `transcode.rs` (417 строк, 3 темы) → `codec.rs` (`Codec`, `VaInfo`, `choose_target`, `available_encoders` — выбор цели), `transcode.rs` (`prepare`, `source_codec`, `source_decodes_hardware`, `run_ffmpeg`, `transcode`, `verify_hardware`, `cache_key`, `MediaPlan`).
- `config.rs` → `config.rs` (`Config` + namespace-ы) и `paths.rs` (`default_config_path`, `cache_dir` — пути XDG).
- `main.rs`: `InstanceLock`/`acquire_instance_lock` → `instance.rs`; `main.rs` остаётся индексом (`mod` + `pub use`, алфавитно) и точкой входа.
- `render.rs` (710) остаётся: одна тема (wgpu-рендер), разбивать не по строкам, а по темам; `frame_key` остаётся рядом с единственным вызовом в `Renderer::import`.

**Сущности и дубль** (шаги 2–3 «Ревью»):

- `App`: pub-поля сужаются до приватных — внешних потребителей полей нет (`main.rs` работает только методами); trait-impls в том же файле читают поля напрямую.
- `Renderer::blur_radius` — приватное поле (читается только внутри `render.rs`).
- `retired: Vec<(Arc<GpuFrame>, u64)>` → value-тип `RetiredFrame { frame, generation }` (Debug, Clone, PartialEq — контейнер `Vec`), `drain_retired` читает геттером.
- Размеры `(u32, u32)` в сигнатурах `present`/`draw_live`/`draw_blur`/`configure` → value-тип `Size { width, height }` (Debug, Clone, Copy, PartialEq); кортежи-россыпь из сигнатур уходят.
- `draw_live`/`ensure_blur`/`draw_blur` — три тела одинаковой формы (pass → pipeline → bind → immediates → draw 0..3); различие — пайплайн/бинд-группа/вид/метка — в аргументах одного приватного хелпера `draw_triangle`.
- `configure_entry(key, blur: bool)` → параметр `DrawMode` (bool дублирует существующий enum); `should_draw` — match по `(mode, frame)`.
- `priority::set_game_active(active: bool)` → enum `GameScheduling { Idle, Normal }`, два вызова в `on_game` матчятся.
- `redraw`: коллекция `keys: Vec<ObjectId>` убирается деструктурирующим borrow (`let App { outputs, renderer, .. } = self;` — поля не пересекаются).
- Дубль clamp масштаба: clamp один раз в конструкторе `OutputPair`, `output_scale` удаляется (инвариант уже гарантирует).
- `Control::set_overview_open` возвращает неиспользуемый `bool` — возврат убирается.
- `free_transient_state` (тело — `malloc_trim`) переименовывается по сути — `trim_heap`.

**Поток данных** (шаг 4 «Ревью», находки в review.md):

- **Один путь на потребителя.** `Control` становится контрактом только декодера: поля `game_active`/`shutdown`, писатели — watcher'ы и `App::on_game`, читатель — worker. Поле `overview_open` из `Control` удаляется (читателей не остаётся), `niri.rs` перестаёт писать в control. `App` реагирует только на события calloop-канала; `sync_control_state` и её вызов в `dispatch_loop` удаляются. Идемпотентные guard-ы `on_overview`/`on_game` остаются.
- **`Renderer` — единственный источник истины о валидности блюра.** `ensure_blur` возвращает `()` и держит свой guard по `blur_valid_seq`; предпроверки `blur_valid(seq)` в `configure_entry`/`redraw` удаляются; `blur_valid` остаётся публичным только там, где без него не обойтись (проверка перед показом blur-поверхности без пересчёта), `blur_passes` — как есть.
- **Кадр течёт локально.** `receive_into_pending` → `Result<ReceiveOutcome>`, где `enum ReceiveOutcome { Frame(AvFrame), NeedPacket, Eof }` — готовый кадр значением, EOF вариантом, не строкой-сентинелом; маппинг в `VideoFrame` — локальный в `next_frame`-цепочке; поля `pending`/`mapped` удаляются из `Decoder`.
- `niri::connect_and_stream` возвращает enum `NiriOutcome { Unavailable, StreamEnded }` — внешний цикл матчит исход, а не расшифровывает bool.
- Счётчики-scaffolding удаляются: `impl Drop for VideoFrame` (только статический счётчик) и `HITS` в `Renderer::import`.
- `Decoder::open_inner` — обычная приватная `fn` (unsafe-блоки точечные); `frame_key` — ключ только по inode dmabuf-fd.

**Паника → ошибка** (шаг «Ошибки и инварианты»):

- `Mutex::lock().unwrap()` (decoder/control, mailbox, priority): `lock().unwrap_or_else(std::sync::PoisonError::into_inner)`. Альтернатива `parking_lot` — новая зависимость, запрещена. Отравленный `ControlState` всё равно содержит валидное значение.
- `CString::new(render_node()).unwrap()` в `Decoder::open`: `render_node()` меняет возврат на `Result<CString, ...>`? Нет — узел приходит из env и содержит `NUL` теоретически; `Decoder::open` возвращает `Result` уже сейчас, `CString::new` валидируется через `with_context`. `render_node()` остаётся `String`.
- `self.pending.take().unwrap()` в `map_pending`: контракт «pending уже есть» переоформляется в поток — кадр возвращается значением из `receive_into_pending` (см. «Поток данных»), поля `pending`/`mapped` исчезают из `Decoder`.
- `frame.unwrap()` в `App::configure_entry`: guard `let Some(frame) = self.current.clone() else { return; }` наверху, `should_draw` считается после.
- `blur_targets.as_ref().unwrap()` в `ensure_blur`: `ensure_blur_targets` возвращает `&BlurTargets` (всегда создаёт/оставляет валидную цель), вызов `let targets = self.ensure_blur_targets(...)` — без `Option`-развилки.
- `.expect("cannot spawn ... thread")` в `niri::spawn`/`gamemode::spawn`: `spawn` возвращает `Result<JoinHandle<()>, ...>`; `main.rs` прокидывает в `real_main`. Отказ уже запущенного потока остаётся «продолжаем без наблюдателя» (сценарий спеки).
- `next_frame`-sentinel `"__eof__"`: `match` по `ReceiveOutcome` (см. «Поток данных») вместо `e.to_string() == "__eof__"`; EOF — вариант, а не ошибка.

**Мелочи.** `Renderer::import` — параметр `queue` не используется, убрать. `frame_key` — `planes[0]` → `frame.planes.first()`, инвариант «2 плоскости» ставит `Decoder::map_pending` (bail при `< 2`), `frame_key` возвращает `Result` уже сейчас. Комментарии/doc-комментарии удаляются при переносе — смысл в именах (по AGENTS.md комментарии запрещены); где имя не передаёт смысл, имя переименовывается.

**Проверка.** `direnv exec . chrt -i 0 nice -n 19 ionice -c 3 env CARGO_BUILD_JOBS=10 cargo clippy --all-targets` + `cargo test -j 10` + запуск контейнера по «Среда» (headless Xwayland, скриншот) — визуальный прогон только при наличии GPU; без GPU — компиляция/тесты как подтверждение, ограничение проговаривается.

## Risks / Trade-offs

- `git mv` пофайлово при одновременном разрезании файлов теряет rename-детект для частей — принимается: история читается по коммиту переноса до разрезания (два коммита: чистый перенос, затем правки).
- `unwrap_or_else(into_inner)` на отравленном мьютексе маскирует баг в потокобезопасном состоянии; принято: состояние — `Copy`-структура, отравление не оставляет её невалидной.
- Разделение `app.rs` выносит trait-impls рядом с `App` в `app.rs`, а `SurfaceEntry` — в `surface.rs`; ассоциированные типы SCTK требуют, чтобы impl'ы оставались на `App` — это и делается, ничего не переезжает «за инвариантом».
- Визуальная проверка зависит от hardware GPU; без него `[INFERENCE]`-граница — компиляция и юнит-инварианты.
