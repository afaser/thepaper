## 1. Флейк и пакеты

- [x] 1.1 Перепинить `inputs.nixpkgs.url` на `github:NixOS/nixpkgs/nixos-unstable`, перегенерировать `flake.lock`; проверить `jq -e '[.. | objects | select(.type? == "path")] | length == 0' flake.lock` и `nix flake metadata . --json`, где nixpkgs резолвится как `github`
- [x] 1.2 Ввести общий набор пакетов для `devShell` и пакетов, выбрать один атрибут FFmpeg по фактической сборке `ffmpeg-sys-next` (`ffmpeg` или `ffmpeg_7`) и использовать его везде; проверить `direnv exec . cargo test`, `direnv exec . ffmpeg -version` и совпадение мажора в devShell и `.#source`
- [x] 1.3 Добавить `packages.x86_64-linux.source`: `rustPlatform.buildRustPackage`, `src = self`, `cargoLock.lockFile`, версия из `Cargo.toml`, `bindgenHook`, `pkg-config`, `autoPatchelfHook`, `makeWrapper`, `runtimeDependencies = [ vulkan-loader libglvnd ]`; ELF в `$out/libexec/thepaper`, wrapper в `$out/bin/thepaper` с `ffmpeg` и `libva-utils`; `installCheckPhase` сверяет `--version`; проверить `nix build .#source`, `result/bin/thepaper --version` и наличие обоих инструментов в wrapper
- [x] 1.4 Добавить `nix/release-package.nix` с конструктором `{ version, src }` (ELF из `libexec`, тот же `autoPatchelfHook`, `runtimeDependencies`, wrapper, проверка версии) и `lib.mkReleasePackage`; `default`/`thepaper` строятся через `fetchurl` из `release.json`, при отсутствии файла указывают на `source`; проверить до релиза `nix build .#default` и `nix eval --raw .#default.version`
- [x] 1.5 Добавить `checks.x86_64-linux.source`, `checks.x86_64-linux.default` и `apps.x86_64-linux.default`; проверить `nix flake check` на чистом клоне без `release.json`
- [x] 1.6 Проверить бинарную сборку из локального архива: `nix build --impure --expr 'let f = builtins.getFlake (toString ./.); in f.lib.mkReleasePackage { version = "0.1.0"; src = /путь/thepaper-x86_64-linux.tar.gz; }'`, затем `result/bin/thepaper --version` и RPATH с каталогами `vulkan-loader` и `libglvnd`

## 2. CI

- [ ] 2.1 Создать `.github/workflows/ci.yml` с триггерами `push` в `trunk` и `pull_request`, `concurrency` по ref и `permissions: contents: read`; шаги: checkout, установка Nix, проверка `flake.lock` на `"type": "path"`, `nix flake check --print-build-logs`, `nix build .#default --print-build-logs`, сверка `./result/bin/thepaper --version` с `nix eval --raw .#default.version`, `--help`; проверить зелёный прогон на тестовом pull request без секретов
- [ ] 2.2 Проверить провал: ветка с некомпилируемым `src/` даёт красный workflow на сборке `source`, ветка с подменённым `release.json` — красный на сверке версии; оба изменения откатить

## 3. Релиз и установка

- [ ] 3.1 Создать `.github/workflows/release.yml` на `workflow_dispatch` с входом версии и `permissions: contents: write`: проверка semver и равенства `Cargo.toml`, отказ при опубликованном `vX` (кроме режима доведения пина), сборка `.#source`, детерминированный архив `thepaper-x86_64-linux.tar.gz`, SRI-хеш, локальная проверка через `lib.mkReleasePackage`, запись `release.json`, пин-коммит, тег `vX`, push только тега, draft с `--verify-tag`, загрузка, публикация через `gh release edit vX --draft=false`, проверка публичного URL и `nix build .#default`, fast-forward push пина в ветку запуска
- [ ] 3.2 В workflow реализовать ошибки и повтор: до публикации удалять только созданные этим запуском draft и тег; после публикации ничего не удалять и не пушить при ошибке; повторный запуск опубликованной версии только fast-forward отправляет существующий пин-коммит, при ушедшей ветке падает без force; добавить `concurrency` без отмены; проверить все ветки на тестовом репозитории/ветке
- [ ] 3.3 Запустить release-workflow с `0.1.0` на `trunk`; проверить релиз `v0.1.0`, ассет, `release.json`, тег на пин-коммите, отсутствие force-операций в логе, `nix build .#default` и `nix eval --raw .#default.version`
- [ ] 3.4 На чистом профиле установить `github:afaser/thepaper` и `github:afaser/thepaper/v0.1.0`: в логе есть загрузка ассета и `autoPatchelf`, нет `cargo`/`rustc`, `--help` и `--version` работают
- [ ] 3.5 Проверить негативный сценарий версии: запуск с `9.9.9` при `0.1.0` падает до сборки; проверить повторный запуск `0.1.0` после изменения `src/` — workflow отказывает без перезаписи ассета
- [ ] 3.6 Проверить режим обновления зависимостей на следующей версии: в ветке поднять `Cargo.toml` и `flake.lock`, выпустить `v0.1.1` с этой ветки, убедиться, что `checks.default` зелёный до слияния, тег и ассет не перезаписываются
- [ ] 3.7 Обновить `README.md`: установка из `trunk` и тега без кеша, отставание `default` от `Cargo.toml`, версия артефакта, процесс релиза, обязательный bump версии при обновлении nixpkgs, предусловия прав `GITHUB_TOKEN`/защиты ветки; проверить команды и имена тегов/ассетов

## 4. Интеграционная проверка

- [ ] 4.1 На чистом клоне `trunk` выполнить `nix flake check`, `nix build .#default` и установку из профиля; убедиться, что версия и `--help` совпадают с релизом, а сборка не содержит `cargo`/`rustc`
