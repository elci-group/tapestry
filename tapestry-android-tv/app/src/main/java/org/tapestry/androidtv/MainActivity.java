package org.tapestry.androidtv;

import android.app.Activity;
import android.graphics.Color;
import android.graphics.Typeface;
import android.os.Bundle;
import android.view.Gravity;
import android.view.View;
import android.widget.Button;
import android.widget.LinearLayout;
import android.widget.TextView;

/** D-pad-first local review shell for Android TV. */
public final class MainActivity extends Activity {
    private final ReviewState state = new ReviewState();
    private LinearLayout root;
    private boolean focusedActionAdded;

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);
        showHome();
    }

    private void prepareScreen(String title) {
        focusedActionAdded = false;
        root = new LinearLayout(this);
        root.setOrientation(LinearLayout.VERTICAL);
        root.setGravity(Gravity.CENTER_VERTICAL);
        root.setPadding(72, 48, 72, 48);
        root.setBackgroundColor(Color.rgb(16, 20, 34));
        root.setFocusableInTouchMode(true);
        setContentView(root);

        TextView heading = new TextView(this);
        heading.setText(title);
        heading.setTextColor(Color.WHITE);
        heading.setTextSize(34);
        heading.setTypeface(Typeface.DEFAULT, Typeface.BOLD);
        root.addView(heading, matchWrap());
    }

    private void showHome() {
        prepareScreen("Tapestry for Android TV");
        addText("TV review endpoint", 23, Color.rgb(173, 190, 255));
        addText("Local demonstration mode. No desktop is connected.", 19, Color.LTGRAY);
        addText("Use the remote's directional pad and Select button.", 18, Color.LTGRAY);
        addButton("Start local review demo", view -> {
            state.beginLocalDemo();
            showReview();
        });
    }

    private void showReview() {
        prepareScreen("Review sample");
        addText("Sample artefact 1 of 1", 22, Color.rgb(173, 190, 255));
        addText("A quiet evening landscape", 28, Color.WHITE);
        addText("Preview placeholder — no image has been received.", 19, Color.LTGRAY);
        addText("This local demo does not send a decision to another device.", 18, Color.LTGRAY);
        addButton("Approve locally", view -> decide(ReviewState.Decision.APPROVED));
        addButton("Reject locally", view -> decide(ReviewState.Decision.REJECTED));
    }

    private void decide(ReviewState.Decision decision) {
        if (state.decide(decision)) {
            showResult();
        }
    }

    private void showResult() {
        prepareScreen("Local review complete");
        String result = state.decision() == ReviewState.Decision.APPROVED
                ? "Approved on this TV"
                : "Rejected on this TV";
        addText(result, 28, Color.WHITE);
        addText("Demo result only. Nothing was sent or persisted.", 19, Color.LTGRAY);
        addButton("Return to home", view -> {
            state.returnHome();
            showHome();
        });
    }

    private void addText(String text, float size, int color) {
        TextView view = new TextView(this);
        view.setText(text);
        view.setTextColor(color);
        view.setTextSize(size);
        view.setPadding(0, 10, 0, 10);
        root.addView(view, matchWrap());
    }

    private void addButton(String label, View.OnClickListener listener) {
        Button button = new Button(this);
        button.setText(label);
        button.setTextSize(19);
        button.setFocusable(true);
        button.setOnClickListener(listener);
        LinearLayout.LayoutParams params = matchWrap();
        params.topMargin = 14;
        root.addView(button, params);
        if (!focusedActionAdded) {
            button.requestFocus();
            focusedActionAdded = true;
        }
    }

    private LinearLayout.LayoutParams matchWrap() {
        return new LinearLayout.LayoutParams(
                LinearLayout.LayoutParams.MATCH_PARENT,
                LinearLayout.LayoutParams.WRAP_CONTENT);
    }

    @Override
    public void onBackPressed() {
        if (state.screen() != ReviewState.Screen.HOME) {
            state.returnHome();
            showHome();
        } else {
            super.onBackPressed();
        }
    }
}
