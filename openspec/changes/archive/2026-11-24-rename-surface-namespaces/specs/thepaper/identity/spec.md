## MODIFIED Requirements

### Requirement: Идентификаторы поверхностей

Префикс идентификаторов layer-shell поверхностей по умолчанию ДОЛЖЕН быть `thepaper`, если в конфигурации не задан другой `app_id_prefix`. Namespace-ы ДОЛЖНЫ иметь вид `<prefix>-<output>` для фоновой поверхности и `<prefix>-overview-<output>` для поверхности овервью.

#### Scenario: Namespace-ы по умолчанию

- **WHEN** `app_id_prefix` не задан в конфигурации
- **THEN** фоновая поверхность выхода `DP-1` имеет namespace `thepaper-DP-1`, а поверхность овервью — `thepaper-overview-DP-1`

#### Scenario: Переопределение префикса

- **WHEN** в конфигурации задан `app_id_prefix = "custom"`
- **THEN** поверхности выхода `DP-1` имеют namespace-ы `custom-DP-1` и `custom-overview-DP-1`
