# Nara Authoring and Editor

These terms distinguish authored game data from the editor surfaces used to work on it.

## Language

**Authoring document**:
A Scene or Prefab source document whose accepted state, history and saved content belong to one editing session.
_Avoid_: Panel, render view.

**Creation template**:
A complete initial component value offered for explicit authoring creation; it does not describe how an incomplete stored record is decoded.
_Avoid_: Missing-field default.

**Editor panel**:
A tool surface for inspecting, editing or presenting project information, such as the Inspector, Project Browser or Scene View.
_Avoid_: Authoring document.

**Panel instance**:
One open occurrence of an editor panel with its own presentation state and target.
_Avoid_: Panel kind, document session.

**Editor workspace layout**:
The arrangement and visibility of editor panel instances across the user's editor windows.
_Avoid_: Scene hierarchy.

**Scene View**:
An editor view of the authored scene with editor camera and manipulation tools.
_Avoid_: Game View, native window.

**Game View**:
A presentation of a running game's output, whether embedded or supplied by an external game process.
_Avoid_: Scene View, dock window.
