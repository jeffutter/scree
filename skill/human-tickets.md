`next` hides tickets assigned to a human and reports `human_pending: N`. Never start one: scree refuses with exit 4. To hand your own ticket to a human, comment what is needed, then reassign:

    scree comment add SCR-12 "Needs a decision on the schema"
    scree edit SCR-12 --for human

Report `human_pending` to the user at the end of the session instead of polling for it.
