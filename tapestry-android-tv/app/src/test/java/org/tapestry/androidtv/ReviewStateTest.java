package org.tapestry.androidtv;

/** Host-runnable assertions for the local review state machine. */
public final class ReviewStateTest {
    public static void main(String[] args) {
        ReviewState state = new ReviewState();
        require(state.screen() == ReviewState.Screen.HOME, "initial screen is home");
        require(!state.decide(ReviewState.Decision.APPROVED), "cannot decide before a review");

        state.beginLocalDemo();
        require(state.screen() == ReviewState.Screen.REVIEW, "demo opens review screen");
        require(!state.decide(ReviewState.Decision.NONE), "empty decision is rejected");
        require(state.decide(ReviewState.Decision.APPROVED), "approval is accepted locally");
        require(state.screen() == ReviewState.Screen.RESULT, "decision opens result screen");
        require(state.decision() == ReviewState.Decision.APPROVED, "approval is recorded");
        require(!state.decide(ReviewState.Decision.REJECTED), "result cannot be overwritten");

        state.returnHome();
        require(state.screen() == ReviewState.Screen.HOME, "return home resets the screen");
        require(state.decision() == ReviewState.Decision.NONE, "return home clears local result");

        state.beginLocalDemo();
        require(state.decide(ReviewState.Decision.REJECTED), "rejection is accepted locally");
        require(state.decision() == ReviewState.Decision.REJECTED, "rejection is recorded");
        System.out.println("ReviewStateTest: all assertions passed");
    }

    private static void require(boolean condition, String message) {
        if (!condition) {
            throw new AssertionError(message);
        }
    }
}
