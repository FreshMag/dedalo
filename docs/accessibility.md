# Accessibility of the web editor

This is the record of an audit of the web editor against WCAG 2.2, level AA,
done in November 2026, and of what was changed because of it.

## How it was checked

- The markup, styles and scripts of the editor were read against each
  success criterion that applies to a single-page tool.
- Colour contrast was computed from the colours in `web/style.css`.
- The keyboard behaviour and the names of controls are checked on every
  change by the editor's self-test (`web/selftest.js`), which drives the
  page in a browser.

It was not tested with a screen reader by a person, nor with users who rely
on assistive technology. That is the main gap; see What is left.

## What was found and fixed

| Finding | Criterion | Fix |
| --- | --- | --- |
| Problems and outline entries were list items with a click handler: reachable by Tab, but not announced as something to activate | 4.1.2 Name, Role, Value | Each entry is a button; a fix is a second button beside it |
| The panels' tabs took three tab stops and ignored the arrow keys; panels were not tied to their tabs | 2.1.1 Keyboard, 4.1.2 | One tab stop; Left, Right, Home and End move; each panel is labelled by its tab and can take focus |
| Tab indents inside the editor. Escape then Tab left it, but nothing said so | 2.1.2 No Keyboard Trap | The way out is written under the editor and is the editor's description for screen readers |
| Help for a word appeared only on mouse hover | 2.1.1 Keyboard, 1.4.13 Content on Hover or Focus | Control+I shows it for the word at the cursor; Escape closes it; it is announced as a status |
| The completion list was not tied to the text area | 4.1.2 | The text area names the list and points at the chosen item while the list is open |
| The preview could be moved and zoomed only with a pointer | 2.1.1 Keyboard | The preview takes focus: arrow keys move it, plus and minus zoom, 0 fits |
| The map had no text alternative | 1.1.1 Non-text Content | The drawing is one labelled image, and its description points to the Text map panel, which gives the same map as text |
| Choosing an example or a download format in a list acted at once, so arrow keys in the list loaded an example or started a download | 3.2.2 On Input | The list only chooses; a Load or Download button acts |
| Borders of buttons and lists were 1.5:1 against the page | 1.4.11 Non-text Contrast | Borders use a colour at 6.2:1 |
| No way past the toolbar | 2.4.1 Bypass Blocks | A skip link to the editor is the first stop |
| The page said it was English whatever it showed | 3.1.1 Language of Page | The page's language follows the chosen interface language; the language names in the picker are marked with their own language |
| Squiggles under errors lost their colour in forced-colours mode | 1.4.1 Use of Color, 1.4.11 | System colours are used in forced-colours mode. Errors are never colour alone: each also appears in the Problems panel and in the status line |
| Plugin messages appeared only in the toolbar, behind the open dialog | 4.1.3 Status Messages | The dialog has its own status line |

## What already held

- Every control has a visible label, and a focus ring of 2 px at 6.9:1.
- Text contrast on the page: body text 17.2:1, muted text 6.2:1, error
  6.5:1, warning 5.9:1, success 6.5:1. In the editor, on its grey
  background, the weakest syntax colour (comments) is 4.6:1.
- The status line is a polite live region, so the count of problems is
  announced after each change.
- The layout reflows to one column under 760 px.
- Nothing moves, blinks or times out. The plugin watchdog stops a plugin,
  not the user.
- The Plugins dialog is a native dialog: it takes focus, keeps it, and
  returns it on close.
- Buttons and lists are at least 28 px tall.

## What is left

- **Testing with a screen reader and with users.** The roles and names are
  right by inspection and by test, but how the editor sounds in practice is
  unknown. The code editor is a plain text area with a painted overlay,
  which screen readers read well, though errors are announced through the
  Problems panel and the status line, not inline.
- **Click-to-source on the map** needs a pointer. The Outline panel does the
  same from the keyboard: each entry goes to its block.
- **Zoom and reflow** were checked by reading the styles, not on devices:
  the toolbar wraps and the panes stack, but 400% zoom was not tried.
- **Only a light theme.** There is no dark mode; forced-colours mode works.
- **The reference pages** are generated from Markdown and were not audited
  beyond their headings and landmarks.
- **Maps themselves.** A map is a picture. Its text alternative is the text
  map, which carries rooms, walls, doors and elements but not names or
  notes; `dedalo inspect` lists those.
