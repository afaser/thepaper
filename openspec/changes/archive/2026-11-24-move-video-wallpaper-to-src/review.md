# Ревью исходников по алгоритму AGENTS.md (до переноса)

Объект: `crates/video-wallpaper/src/*.rs` + `Cargo.toml` крейта. Каждый шаг — записанный результат. Находки folding-утся в `design.md`/`tasks.md`.

## Шаг 1. Файлы

Разложение типов и свободных функций по темам (одна фраза на тему):

- `main.rs` — Cli (CLI); InstanceLock + acquire_instance_lock (одиночный инстанс); real_main (сборка запуска). Логика лока в `main.rs` — правило «`main.rs` — точка входа без логики» нарушено → `instance.rs` (InstanceLock, acquire_instance_lock).
- `compositor.rs` — пуст (одна строка doc-комментария), `mod compositor;` мёртвый → удалить.
- `config.rs` — Config + load/namespace-ы (конфиг); default_config_path, cache_dir (пути XDG). Две темы → `paths.rs` (default_config_path, cache_dir).
- `decoder.rs` — PlaneFormat/DmaPlane/ColorInfo/VideoFrame (кадр для GPU-импорта); AvFrame/FormatCtx/CodecCtx/HwDevice/Packet/get_format/averr + Decoder + render_node (VA-API декодер); Control/ControlState/FrameMsg/Mailbox (контракт декодер↔цикл); Worker/spawn_thread (жизненный цикл потока); free_transient_state (malloc_trim). Пять тем → `decoder.rs` (декодер+FFI), `frame.rs` (кадр), `control.rs` (контракт), `worker.rs` (поток).
- `app.rs` — SurfaceEntry/OutputPair/DrawMode (поверхность выхода); App + trait-impls (состояние приложения); handle_niri_event/handle_game_event/install_sources/dispatch_loop (проводка calloop). Три темы → `surface.rs`, `app.rs`, `sources.rs`.
- `render.rs` — Renderer и пайплайны (рендер); surface_from_wl (wgpu-поверхность из Wayland). surface_from_wl читает только чужие поля, единственный вызов — `SurfaceEntry::new` → перенести в `surface.rs`.
- `transcode.rs` — VaInfo/Codec/choose_target/available_encoders (выбор кодека); source_codec/source_decodes_hardware/run_ffmpeg/transcode/verify_hardware/prepare/MediaPlan (подготовка и исполнение); cache_key (ключ кеша). Три темы → `codec.rs` (выбор кодека), `transcode.rs` (исполнение + кеш).
- `niri.rs` — одна тема (IPC-наблюдение niri). Не режется.
- `gamemode.rs` — одна тема (gamemode D-Bus). Не режется.
- `priority.rs` — одна тема (приоритеты потоков). Не режется.
- `main.rs` после выделений — только `mod` + `pub use` (алфавитно) и `main`.

Ответ на «чьи поля читает функция»: surface_from_wl — чужие, ни разу self → свободная функция файла-потребителя (`surface.rs`); frame_key — вызывается только из `Renderer::import`, остаётся в `render.rs`.

## Шаг 2. Сущности

- InstanceLock — инвариант «лок захвачен» ставит acquire_instance_lock, приватное поле — корректен; получает свой файл.
- VideoFrame — инвариант «fds живы, пока значение живо» держат приватные `_va`/`_drm`; pub-поля данных допустимы (value-часть). Корректен.
- App — все поля `pub`, но внешних потребителей полей нет: `main.rs` работает только методами (`ensure_all_outputs`, `set_decoder`, `shutdown_decoder`, …). Тип с инвариантами → поля приватные; trait-impls в том же файле читают напрямую. **Находка: сузить pub-поля App.**
- Renderer — `pub blur_radius` читается только внутри `render.rs`. **Находка: поле приватное.**
- `retired: Vec<(Arc<GpuFrame>, u64)>` — роль «кадр, поданный в поколении N» без имени (кортеж .0/.1). **Находка: именованный value-тип `RetiredFrame { frame, generation }`.**
- Размеры `(u32, u32)` (SurfaceEntry.size, out_size/present/draw_live/draw_blur, logical/physical) — связанные аргументы-россыпь. **Находка: value-тип `Size { width, height }`** (Debug, Clone, Copy, PartialEq).
- Control/Mailbox/Worker/Decoder/SurfaceEntry — инвариант ставит конструктор, поля приватные — корректны.
- Имена новых типов (`Size`, `RetiredFrame`, `InstanceLock`) с существующими `pub struct/enum` подсистемы не совпадают.

## Шаг 3. Дубль и состояние

- `draw_live` / `ensure_blur` (два прохода) / `draw_blur` — три тела одинаковой формы (begin pass → set pipeline → bind group → immediates → draw 0..3), различие в пайплайне/бинд-группе/виде/метке. **Находка: один приватный хелпер `draw_triangle(...)`; тела прогонов — один вызов.**
- `configure_entry(key, blur: bool)` + `mode = if blur {…}` — bool-флаг дублирует существующий `DrawMode`. **Находка: передавать `DrawMode`, матч вместо флага.**
- `priority::set_game_active(active: bool)` — поведенческая развилка по bool. **Находка: enum `GameScheduling { Idle, Normal }`, матч.**
- `keys: Vec<ObjectId>` в `redraw` — вспомогательная коллекция обхода из-за borrow; отражает `outputs`, но обход решается деструктурирующим borrow (`let App { outputs, renderer, … } = self;`). **Находка: убрать keys.**
- Дубль clamp масштаба: `info.scale_factor.max(1)` при создании пары и `output_scale` (`pair.scale.max(1)`) у потребителей. Инвариант уже ставит конструктор. **Находка: clamp один раз в конструкторе `OutputPair`, `output_scale` удалить.**
- `frame.unwrap()` в `configure_entry`, `blur_targets.as_ref().unwrap()` — учтено (шаг 5, guards).

## Шаг 4. Поток данных

Направление «вход → фазы → результат» формально односторонее (`prepare` → `Decoder::open` → `App` → цикл, владение по значению), **но** один факт течёт параллельными путями, а промежуточное состояние размазано по `self`. Находки:

- **Двойной путь одного факта.** Каждый watcher публикует состояние дважды: пишет `Control` напрямую и шлёт сообщение в calloop-канал. `App` потребляет оба пути — обработчики канала (`handle_niri_event`/`handle_game_event`) и опрос `sync_control_state()` в каждом обороте цикла — и сводит их идемпотентными guard-ами. Это два канала доставки одного события с последующей сверкой. **Исправление: один путь на потребителя.** `Control` — контракт только декодера (`game_active`, `shutdown`); `App` реагирует только на события канала; `sync_control_state` удаляется, поле `overview_open` из `Control` — тоже (после этого читателей у него нет; `niri.rs` перестаёт писать в control).
- **Дубль guard-а блюра.** `ensure_blur` уже no-op при валидном seq, но `configure_entry` (435) и `redraw` (482) предварительно проверяют `blur_valid(seq)` и игнорируют возвращаемый bool — проверка сделана дважды, а в `on_frame` (466) и `on_overview` (584) — просто вызов без проверки. Три стиля вызова одной операции. **Исправление: `ensure_blur` возвращает `()` (свой guard внутри), предпроверки `blur_valid` из `App` удаляются; `blur_valid_seq` остаётся единственным источником истины внутри `Renderer`.**
- **Промежуточное состояние, текущее через `self`.** `Decoder::next_frame` → `receive_into_pending` кладёт кадр в `self.pending` → `map_pending` забирает `self.pending.take().unwrap()`; `self.mapped` заменяется свежим `av_frame_alloc()` каждый кадр. Кадр течёт через три поля `self` вместо локального потока. **Исправление: `receive_into_pending` возвращает `Option<AvFrame>`; mapped-кадр — локальная переменная `map_pending`; поля `pending`/`mapped` из `Decoder` исчезают.**
- **`connect_and_stream -> bool` — три исхода в bool.** «Соединения нет», «стрим закрыт» и «отправка сломалась» кодируются true/false, контракт читается только по комментарию. **Исправление: enum `NiriOutcome { Unavailable, StreamEnded }`, внешний цикл матчит.**
- **`Drop`-только-для-счётчика.** `VideoFrame::drop` не освобождает ничего — поля RAII и так дропаются; тело — статический счётчик для debug-лога. Аналогичный `HITS` в `Renderer::import`. **Исправление: оба счётчика и `impl Drop for VideoFrame` удаляются.**
- **`unsafe fn open_inner`** — всё тело становится unsafe-контекстом; точечные `unsafe`-блоки уже есть внутри. **Исправление: обычная приватная `fn open_inner`.**
- **`frame_key` — вырожденный кортеж**: `(inode, planes.len())`, где `planes.len()` всегда 2 (инвариант `map_pending`). **Исправление: ключ — только inode.**
- `Worker::resume` (decoder течёт thread → канал → worker → новый thread) — обход запрета unprivileged `SCHED_IDLE → SCHED_OTHER`; путь вынужденный, остаётся, контракт фиксируется именем `resume`.
- Между фазами текут результаты (`MediaPlan`, `VideoFrame`, `GpuFrame`) — сущности; `fatal: Option<String>` — собственное состояние `App`, опрашиваемое его же циклом, не боковой канал.

## Шаг 5. Тело

- `Decoder::next_frame`: sentinel `Err(anyhow!("__eof__"))` + `e.to_string() == "__eof__"` — развилка по строке ошибки. **Находка: `enum DecodeStep { Ready, NeedPacket, Eof }` как результат `receive_into_pending`, EOF — вариант, не ошибка; `match`.**
- `Decoder::map_pending`: `for li in 0..desc.nb_layers as usize` / `for pi in 0..layer.nb_planes as usize` — индексные циклы по сырым слайсам FFI. **Находка: итераторы по срезам (`desc.layers[..nb_layers].iter()`); `break` по «уже 2 плоскости» — guard-выход.**
- `open_inner`: сложное выражение в tracing (вложенные `CStr::from_ptr((*dec.0).codec.as_ref()…unwrap_or_default())`). **Находка: именованный `let decoder_name = …`.**
- `map_pending`: `self.pending.take().unwrap()` — контракт «pending есть» не читается по именам. **Находка: `receive_into_pending` отдаёт `Option<AvFrame>`, поле `pending` исчезает.**
- `App::configure_entry`: `frame.unwrap()` после match по `&frame` — guard `let Some(frame) = … else { return; }` наверху; вложенность поднимается guard-ами.
- `Renderer::ensure_blur`: `self.blur_targets.as_ref().unwrap()` — `ensure_blur_targets` возвращает `&BlurTargets` (всегда создаёт/оставляет валидную).
- `Renderer::import`: параметр `queue` не используется (`let _ = queue;`). **Находка: удалить.**
- `frame_key`: `frame.planes[0]` — индексная паника; инвариант «2 плоскости» ставит `map_pending`, но чтение — `planes.first()`. 
- Паники (`Mutex::lock().unwrap()`, `CString::new(...).unwrap()`, `expect` в spawn) — в design.md, сценарий спеки «Отказ при запуске без паники».
- `free_transient_state` — имя обещает освобождение demuxer-состояния, тело — `malloc_trim(0)`. **Находка: переименовать по сути (`trim_heap`), вызвать из `on_game` рядом с `drain_retired`.**

## Шаг 6. Прогон

Планирование правками кода не заканчивалось — прогон входит в имплементацию (tasks.md §4): `cargo clippy --all-targets`, `cargo test`, контейнерный визуальный прогон по «Среда». Границы для новых тестов: пустой вход (0-размерная конфигурация поверхности — уже guard), граница условия (blur_radius 1..=64 — есть в Config::load), consuming (Worker::shutdown, handoff).
