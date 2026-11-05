package org.tapestry.androidtv;

/** Small, deterministic state model for the local TV review demonstration. */
public final class ReviewState {
    public enum Screen {
        HOME,
        REVIEW,
        RESULT
    }

    public enum Decision {
        NONE,
        APPROVED,
        REJECTED
    }

    private Screen screen = Screen.HOME;
    private Decision decision = Decision.NONE;

    public Screen screen() {
        return screen;
    }

    public Decision decision() {
        return decision;
    }

    public void beginLocalDemo() {
        screen = Screen.REVIEW;
        decision = Decision.NONE;
    }

    /** Records a local demo decision. It is never sent to a desktop service. */
    public boolean decide(Decision value) {
        if (screen != Screen.REVIEW || value == Decision.NONE) {
            return false;
        }
        decision = value;
        screen = Screen.RESULT;
        return true;
    }

    public void returnHome() {
        screen = Screen.HOME;
        decision = Decision.NONE;
    }
}
