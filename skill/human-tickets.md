`next` hides tickets assigned to a human and reports `human_pending: N`. Never start one: scree refuses with exit 4. Reassign to a human only for a real human task: work only a person can do, such as supplying credentials, giving an approval or taking a physical step. Comment what is needed, then reassign:

    scree comment add SCR-12 "Needs the production API key from the owner"
    scree edit SCR-12 --for human

A question or a choice is not a human task: raise a Decision instead (see above) and keep working elsewhere.

Report `human_pending` to the user at the end of the session instead of polling for it.
