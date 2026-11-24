## 1. Зависимости и индекс модулей

- [x] 1.1 `Cargo.toml`: удалить `toml = "0.9"` и `serde = { version = "1.0", features = ["derive"] }`, добавить `kdl = { version = "~6.5.0", default-features = false }`; проверка — `direnv exec . cargo tree -p kdl --depth 0` печатает `kdl v6.5.*`, `rg -n '^(toml|serde) =' Cargo.toml` пусто
- [x] 1.2 `src/main.rs`: добавить приватный `mod config_document;` в алфавитном порядке между `config` и `control`, без `pub use`; проверка — `rg -n '^mod ' src/main.rs` сохраняет алфавитный порядок

## 2. Документ KDL

- [x] 2.1 Создать `src/config_document.rs` с `ConfigDocument`: владеет `KdlDocument`, конструктор `ConfigDocument::new(&str)` парсит KDL v2 и отклоняет неизвестные имена, повторные узлы, узлы без единственного аргумента, узлы со свойствами или дочерним блоком; метод `value(&str) -> Option<&KdlValue>` отдаёт аргумент; проверка — `direnv exec . cargo check` без ошибок
- [x] 2.2 Тесты `src/config_document.rs` на границах: полный плоский документ принимается, комментарии допускаются, неизвестная опция отклоняется, повтор опции отклоняется, два аргумента/свойство/дочерний блок отклоняются; проверка — `direnv exec . cargo test config_document`

## 3. Семантика конфигурации

- [x] 3.1 Переписать `src/config.rs`: поля `Config` приватные, геттеры `video_path()`, `blur_radius()`, `app_id_prefix()`, `transcode_codec()`; `Config::from_document` извлекает значения через `ConfigDocument` (`as_string`, `as_integer` + `u32::try_from`), ставит дефолты 12 и `thepaper`, проверяет непустой `video_path`, непустой `app_id_prefix`, диапазон `1..=64` и существование файла видео; `Config::load` читает файл и добавляет контекст `invalid config <path>`; проверка — `direnv exec . cargo check`
- [x] 3.2 Перевести потребителей на геттеры: `src/main.rs` (лог и `prepare`) и `src/app.rs` (`Renderer::new`); проверка — `rg -nP 'config\.(video_path|blur_radius|app_id_prefix|transcode_codec)\b(?!\s*\()' src` пусто, `direnv exec . cargo check` без ошибок
- [x] 3.3 Тесты `src/config.rs`: полный конфиг даёт заданные значения; только `video_path` даёт дефолты; `blur_radius` 1 и 64 принимаются, 0 и 65 отклоняются; отсутствие `video_path`, пустой `video_path`, несуществующий файл и неверный тип аргумента отклоняются с сообщением об опции; проверка — `direnv exec . cargo test config`

## 4. Путь и документация

- [x] 4.1 `src/paths.rs`: путь конфигурации `config.toml` → `config.kdl`; проверка — `rg -n 'config\.kdl' src/paths.rs` находит путь, `rg -n 'config\.toml' src` пусто
- [x] 4.2 `README.md`: путь по умолчанию `config.kdl`, пример в KDL-блоке без `=`, блок миграции «`mv config.toml config.kdl` и убери `=`»; TOML-блока в README не остаётся; проверка — `rg -n 'config\.toml' README.md` находит совпадения только в блоке миграции
- [x] 4.3 Пройти `rg -n 'config\.toml|toml::|serde::Deserialize' src Cargo.toml`; убрать оставшиеся упоминания формата вне архивных изменений; проверка — команда пуста, а в `README.md` `config.toml` остаётся только в блоке миграции

## 5. Прогон и ревью

- [x] 5.1 Отформатировать и прогнать проверки с минимальным приоритетом и одной тяжёлой задачей за раз: `chrt -i 0 nice -n 19 ionice -c 3 direnv exec . cargo fmt --check`, затем `... cargo clippy --all-targets`, затем `... cargo test`; проверка — все три команды успешны, новых предупреждений нет
- [x] 5.2 Запустить бинарь в контейнере по разделу «Среда» с KDL-конфигом, содержащим неизвестную опцию, и убедиться, что диагностика называет путь файла и опцию, процесс завершается ненулевым кодом до подключения к Wayland; проверить валидный KDL-конфиг на отсутствие ошибки разбора (без hardware GPU визуальный прогон не выполняется, граница фиксируется в отчёте)
- [x] 5.3 `openspec validate config-to-kdl --strict`; проверка — изменение валидно
- [x] 5.4 Пройти алгоритм «Ревью» AGENTS по каждому изменённому файлу: выписать типы и функции и разложить по темам, проверить новый тип на инвариант и геттеры, отсутствие дублей/флагов, направление потока «файл → `ConfigDocument` → `Config`», тело функций (guard → шаги) и имена тестов; проверка — записанный результат по каждому шагу, найденные признаки исправлены
