## Translation contribution

**Language:** <!-- e.g. Turkish (tr) -->
**Locale file:** <!-- e.g. locales/tr.json -->
**New language or update to existing?** <!-- New / Update -->

<!-- If this is an update, briefly describe what was changed and why. -->

## Checklist

- [ ] My locale file is named correctly: `locales/<code>.json` using a lowercase BCP 47 tag (e.g. `tr`, `pt_BR`, `zh_CN`)
- [ ] I translated all values from `locales/en.json` - no keys are missing or renamed
- [ ] I did not change any key names - only the values on the right-hand side of each entry
- [ ] I validated the JSON is well-formed: `python3 -c "import json; json.load(open('locales/<code>.json'))"`
- [ ] I kept `{{placeholder}}` variables exactly as they appear in the English strings (same name, same braces)

## Notes

<!-- Optional. Anything the reviewer should know - dialect choices, strings you were unsure about,
     regional variants, etc. -->
