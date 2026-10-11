# Spec Delta

## ADDED Requirements

### Requirement: Editor and dialog events
The client SHALL queue events for opening block and minecart editors, and for the test instance status packet, and SHALL forward `show_dialog` and `clear_dialog` from both configuration and play to a dialog channel, resolving registry references to dialog compounds with at most 64 sub-dialogs, and send the answers it receives from that channel as `custom_click_action` or commands.

#### Scenario: Registry reference
- **WHEN** `show_dialog` names a registry id
- **THEN** the channel receives the registry's dialog compound
