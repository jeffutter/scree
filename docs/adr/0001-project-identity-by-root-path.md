# A Project is identified by its root checkout path

A Project is identified by the canonical absolute path of its root checkout, not by a remote URL or a stored id file. Nothing is written into the repository being worked on, so an id file is ruled out, and a remote URL is missing for local-only repositories and shared by separate clones.

Consequences: two clones of one repository are two Projects with separate Keys. A root moved on disk looks like a new Project until `scree project relink <old> <new>` re-points the old one, which keeps its Key and counter. A Key is never renamed, because ids appear in commits and comments.
