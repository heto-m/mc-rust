# Spec Delta

## Purpose
Screens for writing on signs and in books, and for reading written books, matching vanilla layout, limits and what is sent to the server.

## ADDED Requirements

### Requirement: Sign editor
The system SHALL open a sign editor for `open_sign_editor` showing the sign texture of the wood type, standing, wall or hanging, four lines centered on it in the sign's text color, a cursor and selection on the current line, and a Done button; Up wraps to the previous line, Down and Enter to the next. A line SHALL NOT exceed 90 font pixels on signs and 60 on hanging signs, and closing by Done or Escape SHALL send `sign_update` with the four lines for the opened side.

#### Scenario: Width limit
- **WHEN** a character that would make the current line wider than the limit is typed
- **THEN** the line is unchanged

#### Scenario: Submit
- **WHEN** the editor is closed
- **THEN** one `sign_update` carries the four lines and the screen closes

### Requirement: Book editor
The system SHALL open a book editor for a held writable book showing the page text wrapped to 114 pixels with at most 14 lines and 1024 characters per page, page indicator, previous and next page buttons, PageUp and PageDown, a Sign button and a Done button. Moving past the last page SHALL append an empty page up to 100 pages, and Done or Escape SHALL send `edit_book` without trailing empty pages.

#### Scenario: Line limit
- **WHEN** typing would make the page need a fifteenth line
- **THEN** the page is unchanged

#### Scenario: New page
- **WHEN** the next page button is pressed on the last page
- **THEN** an empty page is appended and shown

### Requirement: Book signing
The system SHALL let the player sign a book by entering a title of at most 15 characters, enabling Sign and Close only for a non-blank title, and sending `edit_book` with the pages and the trimmed title; Cancel SHALL return to editing.

#### Scenario: Blank title
- **WHEN** the title is empty or blank
- **THEN** Sign and Close is disabled and Enter does nothing

### Requirement: Written book viewer
The system SHALL show a written book opened by `open_book` or a lectern with page text, page indicator and page turning by buttons, arrow keys and PageUp and PageDown, and a click on text with a click event SHALL open the matching confirmation for links, commands and copies, and jump for `change_page`; `show_dialog` and `custom` click events in book text SHALL do nothing.

#### Scenario: Link
- **WHEN** a page link is clicked
- **THEN** a confirmation showing the exact URL opens and nothing is opened without confirming

#### Scenario: Page reference
- **WHEN** a `change_page` click event names page 2
- **THEN** the second page is shown
