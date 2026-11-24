## MODIFIED Requirements

### Requirement: Размещение пользовательских данных

Конфигурация по умолчанию ДОЛЖНА читаться из `$XDG_CONFIG_HOME/thepaper/config.kdl`, а при незаданном `XDG_CONFIG_HOME` — из `~/.config/thepaper/config.kdl`; явный `--config` ДОЛЖЕН иметь приоритет. Кеш транскодированного видео ДОЛЖЕН находиться в `$XDG_CACHE_HOME/thepaper/` (фолбэк `~/.cache/thepaper/`). Лок единственного инстанса ДОЛЖЕН находиться в `$XDG_RUNTIME_DIR/thepaper.lock` (фолбэк — каталог кеша). Файл `config.toml` и пути с именем `video-wallpaper` НЕ ДОЛЖНЫ читаться и создаваться. Нормативный уровень — MUST; запреты выше — MUST NOT.

#### Scenario: Конфиг по умолчанию

- **WHEN** приложение запущено без `--config`
- **THEN** конфигурация читается из `$XDG_CONFIG_HOME/thepaper/config.kdl`, а при незаданном `XDG_CONFIG_HOME` — из `~/.config/thepaper/config.kdl`

#### Scenario: Явный путь конфигурации

- **WHEN** приложение запущено с `--config /path/to/config.kdl`
- **THEN** читается указанный файл, а путь по умолчанию не используется

#### Scenario: Кеш транскодированного видео

- **WHEN** исходное видео требует однократного транскодирования
- **THEN** результат кешируется под `$XDG_CACHE_HOME/thepaper/`, а при незаданном `XDG_CACHE_HOME` — под `~/.cache/thepaper/`

#### Scenario: Второй экземпляр

- **WHEN** запускается второй экземпляр при работающем первом
- **THEN** второй экземпляр завершается с диагностической ошибкой, указывающей на лок `$XDG_RUNTIME_DIR/thepaper.lock`, и лок `video-wallpaper.lock` не создаётся
