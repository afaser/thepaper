---
name: release-thepaper
description: Выпустить новую версию thepaper и обновить установку из GitHub. Использовать, когда фикс или фича есть в исходниках, но отсутствует в установленной версии, либо когда нужно опубликовать релиз.
metadata:
  author: thepaper
  version: "1.0"
---

# Выпуск релиза thepaper

`flake.nix` отдаёт `packages.default` из `release.json`: это предсобранный
тарбол, а не сборка из исходников. Поэтому `nix profile install
github:afaser/thepaper` на любом коммите ставит версию из `release.json`.
Исходники попадают к пользователям только через новый релиз.

## 1. Диагностика

```sh
command -v thepaper
readlink -f "$(command -v thepaper)"   # store path и версия
thepaper --version
nix profile list | grep -A4 -E 'Name:[[:space:]]+thepaper'
gh release list
git log --oneline -5
```

Признак «нужен релиз»: фикс есть в `trunk`, но `release.json` и store path
установленного бинаря называют старую версию. Локальная сборка исходников
(`nix build github:afaser/thepaper#source`) проверяет код, но в PATH не
попадает и `release.json` не меняет.

## 2. Подготовка версии

- `Cargo.toml`: `version = "X.Y.Z"` без `v`. Версия обязана совпасть с input
  workflow, иначе preflight упадёт.
- Опубликованные тег и ассет неизменяемы: любое изменение артефакта — новая
  версия, перезапись запрещена.
- `direnv exec . cargo check -j 10` обновит `Cargo.lock` (версия пакета).
- Прогнать `cargo fmt --check`, `cargo clippy --all-targets`, `cargo test`;
  тяжёлые команды — `chrt -i 0 nice -n 19 ionice -c 3 direnv exec . …`.
- Коммит: `chore(release): bump version to X.Y.Z`.

## 3. Push

Интерактивный SSH в агенте часто недоступен. Рабочий вариант — HTTPS с
токеном `gh`:

```sh
gh auth status
git -c credential.helper='!gh auth git-credential' \
  push https://github.com/afaser/thepaper.git trunk
```

Push по URL не обновляет tracking-ref; чтобы `git status` не показывал
«ahead», после push выполнить:

```sh
git -c credential.helper='!gh auth git-credential' \
  fetch https://github.com/afaser/thepaper.git trunk:refs/remotes/origin/trunk
```

## 4. Запуск Release

```sh
gh workflow run release.yml --ref trunk -f version=X.Y.Z
gh run list --workflow=release.yml --limit 3     # взять id запуска
gh run watch <id> --exit-status --interval 15    # 4–5 минут
```

Workflow проверяет `version == Cargo.toml`, собирает `.#source`, публикует
immutable релиз `vX.Y.Z` с ассетом `thepaper-x86_64-linux.tar.gz` и коммитит
`release vX.Y.Z`, обновляя `release.json` (url и hash). Не поллить запуск
вручную и не запускать второй Release параллельно.

## 5. Проверка релиза и пина

```sh
gh release view vX.Y.Z --json tagName,isDraft,assets
git -c credential.helper='!gh auth git-credential' \
  fetch https://github.com/afaser/thepaper.git trunk
git merge --ff-only FETCH_HEAD
git show HEAD:release.json        # version/url/hash = vX.Y.Z
```

Обновить README (`nix profile add github:afaser/thepaper/vX.Y.Z`), коммит
`docs: pin the installation example to vX.Y.Z`, push по схеме из шага 3.

## 6. Обновление установки

```sh
nix profile upgrade --refresh thepaper
readlink -f "$(command -v thepaper)"   # …-thepaper-X.Y.Z/…
thepaper --version
```

`--refresh` обязателен: без него Nix оставляет прежний залоченный ревижн и
пишет `upgrading … to …` с тем же коммитом, а store path не меняется.

Перезапуск обоев: экземпляр один (instance lock). Завершить текущий процесс и
запустить `thepaper` либо перелогиниться — niri поднимет его autostart-ом.
Не завершать чужой экземпляр без разрешения пользователя.

## Ограничения

- Релиз выполняется только из `trunk`.
- Коммит пина может не пройти при branch protection или без `contents:
  write`; тогда workflow падает на push, а тег нужно разбирать по его логу.
- Если после релиза нашлась ошибка в артефакте — bump версии и новый релиз,
  а не перезапись тега.
