## Why

Флейк умеет только `devShell`, а nixpkgs прибит к `path:/nix/store/...` — пакета нет, GitHub Actions такой вход не разрешит. `nix profile add github:afaser/thepaper` должен ставить готовый бинарь с GitHub Releases, без локальной компиляции Rust: Cachix и прочие substituter'ы отклонены, кеш GitHub Actions подстановку не даёт, а релизные ассеты не годятся как бинарный кеш (narinfo ссылается на `nar/<hash>.nar`, вложенных путей у ассетов нет). Поэтому пакет сам скачивает релизный ELF и патчит его под библиотеки из своего nixpkgs.

## What Changes

- Флейк перепинивается на `github:NixOS/nixpkgs` (ревизия в `flake.lock`); `devShell` и пакеты используют один и тот же набор зависимостей, включая один атрибут FFmpeg.
- `packages.x86_64-linux.source` — `rustPlatform.buildRustPackage` с `Cargo.lock`, тестами, `autoPatchelfHook` и обёрткой, кладущей `ffmpeg`, `ffprobe`, `vainfo` в `PATH`; бинарь лежит в `$out/libexec/thepaper`.
- `nix/release-package.nix` — общий конструктор релизного пакета: `fetchurl` архива, `autoPatchelfHook`, `runtimeDependencies` для Vulkan, wrapper. Его использует и `default`, и release-workflow для локальной проверки архива до публикации.
- `packages.x86_64-linux.default` и `.thepaper` — пакет из `release.json` (`version`, `url`, `hash`); версия пакета — версия артефакта, а не текущего `Cargo.toml`. Пока `release.json` нет, `default` собирается из исходников (bootstrap).
- `.github/workflows/ci.yml`: push в `trunk` и pull request — проверка `flake.lock` на `path:`, `nix flake check`, `nix build .#default`, сверка `--version` с версией пакета.
- `.github/workflows/release.yml`: ручной запуск с версией на выбранной ветке — проверка версии, сборка `.#source`, архив ELF, локальная проверка бинарного пакета, пин-коммит, тег, draft-релиз и публикация. Тег не переносится, ассет не перезаписывается; один номер версии — один неизменяемый релиз.
- README: установка из GitHub без кеша, отставание `default` от `trunk`, процесс релиза и выпуска при обновлении nixpkgs.
- Код приложения не меняется; меняется упаковка, доставка и пин окружения.

## Capabilities

### New Capabilities

- `thepaper/distribution`: пакет Nix из исходников и из релизного бинаря, CI-проверки, неизменяемая публикация ассетов и пина версии, установка из флейк-ссылки без компиляции Rust.

### Modified Capabilities

Нет. Поведение `thepaper` как программы не меняется.

## Impact

- `flake.nix`, `flake.lock` — новый вход nixpkgs, `packages.source`, `packages.default`, `lib.mkReleasePackage`, `apps`, `checks`.
- `nix/release-package.nix` — новый конструктор бинарного пакета, общий для установки и проверки релиза.
- `release.json` — новый файл в корне: версия, URL и хеш релизного архива; обновляется release-workflow.
- `.github/workflows/ci.yml`, `.github/workflows/release.yml` — новые файлы; release требует `contents: write` и права пушить тег и ветку.
- `README.md` — разделы установки, версии артефакта и релиза.
- Внешние зависимости: GitHub Releases как источник бинаря и `cache.nixos.org` для библиотек; сторонние кеши не нужны.
- Ограничение: `nix profile add github:afaser/thepaper` ставит бинарь последнего релиза, а не текущий `trunk`; исходники с `trunk` собираются через `.#source`. Любое изменение бинарного пакета, включая обновление nixpkgs, требует новой версии и нового релиза.
