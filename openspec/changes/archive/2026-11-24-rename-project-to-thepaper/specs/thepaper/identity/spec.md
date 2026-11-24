## Purpose

Определяет пользовательские имена проекта: исполняемый файл, расположение конфигурации, кеша и лока единственного инстанса, переменные окружения и префикс идентификаторов layer-shell поверхностей.

## ADDED Requirements

### Requirement: Имя исполняемого файла

Система ДОЛЖНА поставляться как исполняемый файл `thepaper`; автозапуск, установка и примеры документации ДОЛЖНЫ ссылаться на это имя. Прежнее имя `video-wallpaper` НЕ ДОЛЖНО использоваться как имя программы.

#### Scenario: Запуск обоев

- **WHEN** пользователь запускает `thepaper`
- **THEN** запускается демон обоев и стартовое сообщение в логе содержит имя `thepaper`

### Requirement: Размещение пользовательских данных

Конфигурация по умолчанию ДОЛЖНА читаться из `$XDG_CONFIG_HOME/thepaper/config.toml`, а при незаданном `XDG_CONFIG_HOME` — из `~/.config/thepaper/config.toml`; явный `--config` ДОЛЖЕН иметь приоритет. Кеш транскодированного видео ДОЛЖЕН находиться в `$XDG_CACHE_HOME/thepaper/` (фолбэк `~/.cache/thepaper/`). Лок единственного инстанса ДОЛЖЕН находиться в `$XDG_RUNTIME_DIR/thepaper.lock` (фолбэк — каталог кеша). Пути и файлы с именем `video-wallpaper` НЕ ДОЛЖНЫ читаться и создаваться.

#### Scenario: Конфиг по умолчанию

- **WHEN** приложение запущено без `--config`
- **THEN** конфигурация читается из `$XDG_CONFIG_HOME/thepaper/config.toml`, а при незаданном `XDG_CONFIG_HOME` — из `~/.config/thepaper/config.toml`

#### Scenario: Явный путь конфигурации

- **WHEN** приложение запущено с `--config /path/to/config.toml`
- **THEN** читается указанный файл, а путь по умолчанию не используется

#### Scenario: Кеш транскодированного видео

- **WHEN** исходное видео требует однократного транскодирования
- **THEN** результат кешируется под `$XDG_CACHE_HOME/thepaper/`, а при незаданном `XDG_CACHE_HOME` — под `~/.cache/thepaper/`

#### Scenario: Второй экземпляр

- **WHEN** запускается второй экземпляр при работающем первом
- **THEN** второй экземпляр завершается с диагностической ошибкой, указывающей на лок `$XDG_RUNTIME_DIR/thepaper.lock`, и лок `video-wallpaper.lock` не создаётся

### Requirement: Переменные окружения

Фильтр логов ДОЛЖЕН задаваться переменной `THEPAPER_LOG`, а узел DRM render для декодера и транскодера — переменной `THEPAPER_DRM_RENDER_NODE`. Переменные `VIDEO_WALLPAPER_LOG` и `VIDEO_WALLPAPER_DRM_RENDER_NODE` НЕ ДОЛЖНЫ влиять на работу приложения.

#### Scenario: Переопределение уровня логов

- **WHEN** задана переменная `THEPAPER_LOG=debug`
- **THEN** фильтр логов применяется; значение `VIDEO_WALLPAPER_LOG` при этом игнорируется

#### Scenario: Выбор узла DRM render

- **WHEN** задана переменная `THEPAPER_DRM_RENDER_NODE=/dev/dri/renderD129`
- **THEN** декодер и транскодер используют этот узел; значение `VIDEO_WALLPAPER_DRM_RENDER_NODE` при этом игнорируется

### Requirement: Идентификаторы поверхностей

Префикс идентификаторов layer-shell поверхностей по умолчанию ДОЛЖЕН быть `thepaper`, если в конфигурации не задан другой `app_id_prefix`. Namespace-ы ДОЛЖНЫ иметь вид `<prefix>-live-<output>` для фоновой поверхности и `<prefix>-blur-<output>` для поверхности овервью.

#### Scenario: Namespace-ы по умолчанию

- **WHEN** `app_id_prefix` не задан в конфигурации
- **THEN** фоновая поверхность выхода `DP-1` имеет namespace `thepaper-live-DP-1`, а поверхность овервью — `thepaper-blur-DP-1`

#### Scenario: Переопределение префикса

- **WHEN** в конфигурации задан `app_id_prefix = "custom"`
- **THEN** поверхности выхода `DP-1` имеют namespace-ы `custom-live-DP-1` и `custom-blur-DP-1`
