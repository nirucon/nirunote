# NIRUNOTE — agent-roadmap och arbetsinstruktioner

Du arbetar i det kanoniska NIRUNOTE-repot:

```text
~/Git/nirunote
```

Använd **inte** den äldre kopian under
`~/Nextcloud/Projects-Dev/nirunote/nirunote-0.4.0` som källa. Den är en
historisk, separat klon och kan vara smutsig.

Läs hela denna fil innan du ändrar kod. Målet är en **minimalistisk men
funktionellt mycket stark Markdown-editor** med säker lokal lagring, snabb
återhämtning och ett NIRUX-tema som bara aktiveras när appen faktiskt körs i
NIRUX.

Utgångsläge: **NIRUNOTE 0.4.6**, Python + PySide6. Vid skrivandet är
kompilering, core-self-test, GUI-smoke-test och release-preflight gröna.

---

## 1. Uppdrag

Förbättra NIRUNOTE i denna ordning:

1. **Stabilitet och dataintegritet**
2. **Ingen onödig blockering av UI-tråden**
3. **NIRUX-tema, endast när NIRUX upptäcks**
4. **Säker och förutsägbar Markdown-preview**
5. **Daglig skrivar-UX utan visuellt brus**
6. **Arkitektur, test och release**

Lägg inte till funktioner för att “en editor borde ha dem”. Varje ändring ska
lösa ett verkligt skrivflöde och rymmas i den minimalistiska ytan.

---

## 2. Absoluta regler

- **Förstå först, ändra sedan.** Läs berörd kod och befintliga tester.
- **Gör inte en stor omskrivning av `nirunote/app.py`.** Dela upp i små,
  testade steg.
- **En fas per branch/commit/PR.**
- **Rör inte användarens dokument, recovery-state eller inställningar** under
  test. Använd tillfälliga XDG-kataloger och testfiler.
- **Kör aldrig `git reset --hard`, `git clean -fd`, force-push eller
  destruktiva dataoperationer** utan uttryckligt godkännande.
- **Committa aldrig hemligheter**, tokens, SSH-nycklar, serveruppgifter eller
  användarloggar.
- **Inga shell-strängar med användardata.** Använd argv-listor.
- **Ingen nätverksåtkomst i enhetstester.** Mocka externa anrop.
- **Behåll Python 3.12-kompatibilitet**; CI använder 3.12.
- **Ändra inte standardtemat på Debian/Omarchy.** NIRUX-stöd ska vara
  villkorat.
- **Uppdatera tester, CHANGELOG och releasekontrakt** när beteendet ändras.
- **Kör alltid relevant testnivå innan du säger att något är klart.**
- **Om något är oklart: stanna och fråga** i stället för att gissa.

---

## 3. Definition av klar

En ändring är klar först när:

- [ ] `python3 -m compileall -q nirunote gui-smoke-test.py` lyckas.
- [ ] `python3 nirunote/app.py --core-self-test` lyckas.
- [ ] `QT_QPA_PLATFORM=offscreen python3 gui-smoke-test.py` lyckas.
- [ ] `python3 scripts/release-check.py .` lyckas.
- [ ] Eventuellt `python3 nirunote/app.py --spell-self-test` lyckas när
      Hunspell och svensk ordlista finns.
- [ ] Berörda funktioner har nya eller uppdaterade tester.
- [ ] Ingen ny långsam operation har lagts i GUI-tråden.
- [ ] Inga hemligheter eller användardokument har hamnat i git eller loggar.
- [ ] `git diff` innehåller bara avsedda ändringar.

Vid preview-, dialog- eller temaändringar krävs dessutom manuell test i en
riktig desktop-session, med en lång och en kort Markdown-fil.

---

## 4. Kända styrkor att bevara

NIRUNOTE är redan bra på:

- atomiska sparningar via tempfil och `os.replace`
- recovery-state med återupptagning
- extern filändringsdetektion med `QFileSystemWatcher`
- säker, begränsad inline-HTML i preview
- bakgrundskontroll av stavning
- tangentbordsdrivet gränssnitt och kommandopalett
- fokusläge, skrivbredd och typewriter-läge
- test för lång preview och oförändrad scrollposition
- installer som verifierar innan aktiv release byts

Dessa beteenden får inte regressera.

---

## 5. Prioriterade faser

### Fas 0 — Baseline

**Mål:** grön och dokumenterad utgångspunkt.

1. Kör hela testmatrisen.
2. Läs `nirunote/app.py`, `gui-smoke-test.py`, `install.sh` och
   `scripts/release-check.py`.
3. Kontrollera `git status`; rör inte orelaterade ändringar.
4. Skapa en kort lista över kvarvarande blockerande kodvägar. Ändra inget
   ännu.

**Klart när:** baseline är grön och känd.

---

### Fas 1 — Säker och dynamisk temaresolvering

**Mål:** förbereda för NIRUX utan att ändra dagens teman.

Nuvarande kod använder `THEMES[self.theme]` eller
`THEMES[self.window.theme]` på flera ställen:

- `MarkdownHighlighter.highlightBlock()`
- `Main.update_preview()`
- `Main.update_line_highlight()`
- `Main.apply_theme()`

Det gör att ett nytt tema som `Follow NIRUX` kan ge `KeyError` innan det ens
har målats.

Inför en central funktion, exempelvis:

```python
def current_palette(self):
    ...
```

Alla färglookups ska gå genom den. Den ska alltid returnera en komplett
palette med nycklarna:

```text
bg, fg, muted, panel, border, accent, select, line, md
```

Krav:

- Okänt temanamn faller tillbaka till `dark`.
- `self.theme` får inte muteras tyst till ett annat värde utan att
  inställningen uppdateras.
- Ingen färg får hämtas direkt från `THEMES[...]` i målning, preview eller
  line-highlight.
- Lägg till ett test som skapar en okänd tema-sträng och verifierar fallback.

**Klart när:** alla färgvägar går genom samma resolver och testerna är gröna.

---

### Fas 2 — NIRUX-tema, villkorat

**Mål:** NIRUNOTE följer NIRUX-sviten när appen körs i NIRUX, utan att ändra
beteende på Debian eller Omarchy.

#### Upptäckt

NIRUX ska anses tillgängligt om något av följande finns:

- `~/.config/niruwm/appearance.toml`
- `~/.config/niruwm/config.toml` med en NIRU-tema-inställning
- `niructl` i `PATH` och `niructl theme-catalog --json` fungerar
- `~/.config/nirux/theme/kitty.conf`

Visa **`Follow NIRUX`** i tema-menyn och Preferences endast när NIRUX
upptäcks. På andra system ska menyn se ut som idag.

`Follow NIRUX` får vara standard endast om användaren inte redan har ett
explicit temaval. Ett befintligt `theme=dark` ska respekteras.

#### Källor

Läs i denna ordning:

1. `~/.config/niruwm/appearance.toml`
   - `[colors]` overrides
   - `[font]`, `[spacing]`, `[focus]`, `[motion]`
2. `niructl theme-catalog --json`
   - `active`
   - `themes[].colors`
   - `appearance_overrides`
3. `~/.config/nirux/theme/kitty.conf`
   - fallback med ANSI-färger
4. NIRUNOTE `dark`
   - sista reserv

#### Mappning

| NIRUNOTE | NIRUX |
|---|---|
| `bg` | `background` |
| `fg` | `foreground` |
| `muted` | `muted` |
| `border` | `border` |
| `accent` | `focus` / `accent` |
| `panel` | `surface`, annars blandning av bg/fg |
| `select` | `selected`, annars blandning av bg/accent |
| `line` | mycket svag blandning av bg och fg |
| `md` | muted, eventuellt svagt blandad med accent |

Härledda färger ska ha tillräcklig kontrast, även för ljusa teman som
`niru-carl-larsson`.

#### Live-uppdatering

- Lyssna på `config.toml`, `appearance.toml` och `kitty.conf`.
- Använd en separat `QFileSystemWatcher` eller en tydligt avgränsad watcher.
- Debounce 150–300 ms.
- Lägg till filen igen efter atomiska filbyten.
- När temat ändras ska endast färger och målning uppdateras. Dokumenttext,
  markering, cursor och scrollposition ska bevaras.
- `niructl` får inte anropas i GUI-tråden. Läs filer eller kör i worker.

#### Tester

- aktivt mörkt NIRUX-tema
- ljust NIRUX-tema
- `[colors]`-overrides
- saknad/trasig fil
- okänt tema faller tillbaka till `dark`
- live-byte via watcher
- `Follow NIRUX` syns inte när NIRUX saknas

**Klart när:** NIRUX-temabyte syns utan omstart och NIRUNOTE på Debian
fortfarande startar med `dark`/`light`/`satie` exakt som förut.

---

### Fas 3 — Ingen blockerande I/O

**Mål:** skrivandet ska aldrig kännas trögt.

Prioriterade kodvägar:

- `init_spell()` kör `HunspellDictionary.check()` synkront vid start.
- `quick_open()` gör `rglob()` synkront och kan frysa på stora mappar.
- `recover()` skriver hela dokumentet var 2,5 sekund.
- `update_document_stats()` och `update_preview()` kan bli tunga på stora
  dokument.
- `load()` läser hela filen synkront; acceptera för normala filer men sätt
  en gräns och visa tydligt fel för extrema filer.
- Preview-rendering ska vara billig när innehållet inte ändrats.

Rekommenderad lösning:

- återanvänd och utöka befintlig worker-modell
- debounce och cacha
- avbryt gamla sökningar när användaren skriver vidare
- visa progress eller “arbetar” endast när det behövs
- behåll fokus och scrollposition

**Klart när:** ett dokument på flera hundra kilobyte kan skrivas och
förhandsgranskas utan att fönstret fryser.

---

### Fas 4 — Markdown, preview och säkerhet

**Mål:** förutsägbar Markdown utan att göra editorn tung.

Behåll den medvetet lilla renderaren, men förbättra säkerheten:

- Tillåt endast länkscheman `http`, `https` och `mailto` i preview.
- Blockera `javascript:`, `data:` och okända scheman.
- Fortsätt escaping av all HTML utanför den lilla tillåtna inline-subseten.
- Lägg till tester för skadliga länkar och HTML-fragment.
- Undersök om `QTextBrowser.setOpenExternalLinks(True)` ska kombineras med en
  explicit `anchorClicked`-kontroll.

Funktionella förbättringar, i mån av utrymme:

- tabeller
- bilder med lokal, säker sökväg
- bättre kodblock
- export till HTML
- tydligare status för “sparad”, “ändrad” och “återställd”

Inför inte ett stort Markdown-bibliotek utan att väga beroende, säkerhet och
starttid.

**Klart när:** preview är säker, testad och fortfarande snabb.

---

### Fas 5 — Minimalistisk skrivar-UX

**Mål:** fler användbara funktioner utan att editorn blir rörig.

Prioriterat:

1. **Bättre statusrad**
   - sparat/ändrat
   - ord och tecken
   - språk
   - eventuell NIRUX-temaetikett
2. **Säkrare återhämtning**
   - tydlig information när recovery används
   - möjlighet att spara återställt dokument
3. **Quick Open**
   - asynkron
   - avbrytbar
   - tydlig träfflista
4. **Tangentbord**
   - alla dialoger ska ha Escape, Enter och Tab
   - fokus får aldrig fastna
5. **Tillgänglighet**
   - läsbara kontraster
   - tydlig fokusmarkering
   - skärmläsarnamn där Qt tillåter

Lägg hellre funktioner i kommandopaletten och menyn än i nya permanenta
verktygsrader.

**Klart när:** appen känns lugn, snabb och begriplig utan att funktioner
gömts bort.

---

### Fas 6 — Arkitektur och release

**Mål:** enklare underhåll utan beteendeförändringar.

Dela gradvis upp `nirunote/app.py`, exempelvis:

```text
nirunote/
  app.py
  theme.py
  markdown.py
  spell.py
  editor.py
  dialogs.py
```

Regler:

- en modul i taget
- inga beteendeändringar i samma commit
- flytta eller lägg till tester
- behåll CLI (`--version`, `--core-self-test`, `--spell-self-test`,
  `--self-test`, `--scratchpad`, `--new`, `--focus`, `--open`)
- behåll installations- och återställningsflödet

Release:

- uppdatera `VERSION`, `install.sh`, `README.md`, `RELEASE-CONTRACT.md` och
  `CHANGELOG.md`
- kör `python3 scripts/release-check.py .`
- verifiera installer i staging
- tagga först efter grön CI
- publicera release-ZIP som separat asset
- ingen force-push

---

## 6. Testmatris

Kör minst:

```bash
cd ~/Git/nirunote

python3 -m compileall -q nirunote gui-smoke-test.py
python3 nirunote/app.py --core-self-test
QT_QPA_PLATFORM=offscreen python3 gui-smoke-test.py
python3 scripts/release-check.py .
```

Om Hunspell och svensk ordlista finns:

```bash
python3 nirunote/app.py --spell-self-test
```

Vid temaändring:

- testa `dark`, `light` och `satie`
- testa `Follow NIRUX` i NIRUX
- testa att `Follow NIRUX` inte visas på ett system utan NIRUX
- testa ljust NIRUX-tema
- testa live-byte
- testa trasig `appearance.toml`

Vid previewändring:

- kort dokument
- långt dokument
- kodblock
- listor och checkboxes
- externa länkar
- skadliga `javascript:`/`data:`-länkar
- scrollposition vid oförändrad refresh

Vid filändring:

- extern ändring av osparat dokument
- extern borttagning
- Save As efter konflikt
- recovery efter krasch
- BOM och CRLF

---

## 7. Arbetsflöde för agenten

1. Läs `AGENTS.md`, `README.md`, `CHANGELOG.md`, `RELEASE-CONTRACT.md` och
   berörd kod.
2. Kontrollera `git status`; rör inte orelaterade ändringar.
3. Välj en fas och en avgränsad uppgift.
4. Skapa en beskrivande branch.
5. Implementera minsta möjliga ändring.
6. Lägg till eller uppdatera tester.
7. Kör testmatrisen.
8. Granska `git diff`.
9. Committa med tydligt meddelande.
10. Rapportera ändring, testresultat och kvarvarande risker.

Rekommenderade commit-prefix:

```text
fix: ...
feat: ...
perf: ...
test: ...
docs: ...
refactor: ...
```

---

## 8. Stoppvillkor

Stanna och fråga om:

- en ändring kräver att användardokument eller recovery-state raderas
- en testning kräver riktiga hemligheter eller produktionsdata
- en ändring kräver ett stort Markdown-bibliotek eller ny dependency
- en refaktorering kräver att stora delar av `app.py` skrivs om
- en ändring påverkar installer, rollback eller användarens inställningar
- det finns orelaterade lokala ändringar som riskerar att komma med i commit

---

## 9. Startprompt för nästa agent

Kopiera gärna detta som första instruktion:

```text
Läs AGENTS.md och följ den. Börja med Fas 0: kör baseline-testerna och
kartlägg tema- och I/O-vägarna. Ändra ingen funktionalitet i samma steg.
Rapportera testresultat och exakt vad som ändrades. Om arbetskatalogen har
orelaterade ändringar, lämna dem orörda.
```

När Fas 0 är grön är nästa steg **Fas 1: central temaresolvering**, därefter
**Fas 2: Follow NIRUX, endast när NIRUX upptäcks**.
