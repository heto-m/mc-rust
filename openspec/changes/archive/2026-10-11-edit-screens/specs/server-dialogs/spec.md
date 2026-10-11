# Spec Delta

## Purpose
Server-defined dialogs shown in configuration and play, rendered and answered like vanilla with every size and count bounded.

## ADDED Requirements

### Requirement: Dialog data
The system SHALL parse notice, confirmation, multi action, dialog list and server links dialogs with plain message and item bodies and text, boolean, single option and number range inputs, rejecting a dialog whole when it is malformed, pauses without unpausing after-action, or exceeds 32 body entries, 32 inputs, 64 buttons, 64 options, 64 sub-dialogs, 1024-wide controls or 4096-character texts.

#### Scenario: Oversized
- **WHEN** a multi action dialog lists 65 actions
- **THEN** it is rejected and no screen opens

### Requirement: Dialog screen
The system SHALL draw the title with the custom screen warning button, the body and inputs centered and scrollable, action buttons in the footer or a grid of the given columns, button tooltips, and make Escape run the dialog's cancel action unless it cannot be closed with Escape.

#### Scenario: Escape
- **WHEN** Escape is pressed on a dialog that can be closed with it
- **THEN** the cancel action runs and the dialog closes

### Requirement: Actions
Button actions SHALL run the static click event, substitute `$(key)` input values into a command template, or send a custom action carrying the inputs, and then apply the after-action: close, stay, or show the waiting screen. Commands SHALL be sent without a leading slash only in play, `show_dialog` SHALL resolve registry ids and open the dialog, links SHALL go through the link confirmation, and no action SHALL run without a click or key.

#### Scenario: Template
- **WHEN** the template `say $(name)` has an input `name` set to `Ann` and the button is pressed
- **THEN** the command `say Ann` is sent

#### Scenario: Custom action
- **WHEN** a custom action button is pressed
- **THEN** `custom_click_action` carries its id and a compound of the additions and input values

### Requirement: Dialog lifecycle
The system SHALL replace a dialog on top by a newly shown one, close one dialog layer on `clear_dialog`, and show dialogs sent during configuration over the connecting screen without stalling the connection.

#### Scenario: Configuration dialog
- **WHEN** a dialog arrives during configuration
- **THEN** it is shown and its answer is sent in the configuration state

### Requirement: Waiting for response
The waiting screen SHALL show a Back button after one second counting down, usable after five seconds.

#### Scenario: Early escape
- **WHEN** Escape is pressed before five seconds
- **THEN** the waiting screen stays
