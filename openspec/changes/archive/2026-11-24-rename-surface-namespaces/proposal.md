## Why

Форма namespace-ов `-live-`/`-blur-` была выбрана агентом в исходном изменении, а не пользователем: роль поверхности называлась по эффекту рендера (`blur`), а не по месту, где она видна. Пользователь выбрал `thepaper-<output>` для фоновой поверхности и `thepaper-overview-<output>` для поверхности овервью.

## What Changes

- **BREAKING**: namespace фоновой поверхности — `<prefix>-<output>` вместо `<prefix>-live-<output>`; namespace поверхности овервью — `<prefix>-overview-<output>` вместо `<prefix>-blur-<output>`.
- `Config::background_namespace` и `Config::overview_namespace` вместо `live_namespace` и `blur_namespace`; лог создания поверхностей печатает `background=` и `overview=`.
- Правило niri и README переходят на `match namespace="^thepaper-overview-"`.
- Спека `thepaper/identity` фиксирует новую форму и оба сценария.

## Capabilities

### New Capabilities

Нет.

### Modified Capabilities

- `thepaper/identity`: меняется форма идентификаторов layer-shell поверхностей (требование «Идентификаторы поверхностей»).

## Impact

- Код: `src/config.rs`, `src/app.rs`.
- Документация: `README.md`.
- Спеки: `openspec/specs/thepaper/identity/spec.md`.
- Пользовательская среда: правило niri `^thepaper-overview-` (вне репозитория). Старые namespace-ы не поддерживаются.
