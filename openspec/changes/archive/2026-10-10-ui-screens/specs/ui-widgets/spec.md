## ADDED Requirements

### Requirement: Backend independent input
The system SHALL accept mouse move, press, release, scroll, key and character events as `UiInput` in GUI-scaled coordinates, without depending on any windowing crate.

#### Scenario: Scaled coordinates
- **WHEN** the app converts a physical cursor position with the frame scale
- **THEN** widgets hit-test in GUI pixels

### Requirement: Edit box
The system SHALL provide a single-line edit box with cursor, selection, word movement, clipboard through a `Clipboard` trait, a maximum length and a history, and SHALL never hold more than its maximum length.

#### Scenario: Paste over selection
- **WHEN** text is selected and pasted
- **THEN** the selection is replaced and the cursor sits after the pasted text

#### Scenario: Maximum length
- **WHEN** typing or pasting would exceed the maximum length
- **THEN** the text is truncated to the maximum length

### Requirement: Focus navigation
The system SHALL move focus between enabled widgets with Tab and Shift+Tab, wrapping around, and SHALL activate the focused button with Enter or Space.

#### Scenario: Wrap
- **WHEN** Tab is pressed on the last widget
- **THEN** the first enabled widget is focused

### Requirement: Buttons, slider and cycle button
The system SHALL draw buttons from the vanilla widget sprites with normal, highlighted and disabled states, report a click-sound hook on activation, and provide a slider and a cycle button that report value changes.

#### Scenario: Disabled button
- **WHEN** a disabled button is clicked
- **THEN** no action is produced

### Requirement: Tooltips
The system SHALL draw a tooltip from a list of lines using the vanilla tooltip background and frame sprites, placed beside the cursor and clamped inside the screen.

#### Scenario: Screen edge
- **WHEN** the cursor is at the right edge
- **THEN** the tooltip is drawn left of the cursor and fully visible

### Requirement: Scrollable list
The system SHALL provide a fixed-row-height scrolling list model with mouse wheel scrolling, a draggable scrollbar, clamped scroll offsets, hit-testing of rows and keeping a row visible.

#### Scenario: Clamp
- **WHEN** the list is scrolled far past its end
- **THEN** the offset stops where the last row is fully visible

### Requirement: Screen stack
The system SHALL keep a stack of screens where only the top receives input, and screens SHALL return `ScreenAction` values for the app to execute.

#### Scenario: Pop
- **WHEN** the top screen returns `Pop`
- **THEN** the previous screen becomes active
