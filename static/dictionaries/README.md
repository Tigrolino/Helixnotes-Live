# Dictionaries

`en.aff` + `en.dic` - the Hunspell en_US dictionary used by the offline spelling-correction
feature's Basic engine (`src/lib/editor/spellcheck.ts`, via the `nspell` package). This is the
same dictionary format - and, via SCOWL, largely the same underlying wordlist - that powers
spell-check in Word, Chrome, Firefox, and LibreOffice.

Source: the `dictionary-en` npm package (https://github.com/wooorm/dictionaries), which
packages the en_US Hunspell dictionary derived from SCOWL (http://wordlist.sourceforge.net).
License: "(MIT AND BSD)" - see `en-LICENSE.txt` (SCOWL/Kevin Atkinson's license, bundled
verbatim from the `dictionary-en` package) for the full text and attribution requirements.
