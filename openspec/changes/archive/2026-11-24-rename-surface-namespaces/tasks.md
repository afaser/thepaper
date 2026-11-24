## 1. Код

- [x] 1.1 `src/config.rs`: `background_namespace` → `{prefix}-{output}`, `overview_namespace` → `{prefix}-overview-{output}`; проверка — `rg -n -- '-live-|-blur-|live_namespace|blur_namespace' src/` пусто
- [x] 1.2 `src/app.rs`: вызовы новых методов, лог `background=`/`overview=`; проверка — `cargo build` и лог запуска содержит `background=thepaper-DP-1 overview=thepaper-overview-DP-1`

## 2. Документация и конфигурация

- [x] 2.1 `README.md`: правило `^thepaper-overview-` и описание форм `<prefix>-<output>` / `<prefix>-overview-<output>`; проверка — `rg -n -- '-live-|-blur-' README.md` пусто
- [x] 2.2 `~/.config/niri/config.kdl`: `match namespace="^thepaper-overview-"`; проверка — `niri validate -c ~/.config/niri/config.kdl` — valid

## 3. Спека и проверка

- [x] 3.1 `openspec/specs/thepaper/identity/spec.md`: требование и оба сценария на новую форму; проверка — `openspec validate --specs`
- [x] 3.2 `cargo test` и `cargo clippy --all-targets`; проверка — 18 тестов, clippy без замечаний
- [x] 3.3 Запуск на niri: `niri msg layers` показывает `thepaper-DP-1`, `thepaper-overview-DP-1`, `thepaper-HDMI-A-1`, `thepaper-overview-HDMI-A-1`; овервью считает blur-проходы
