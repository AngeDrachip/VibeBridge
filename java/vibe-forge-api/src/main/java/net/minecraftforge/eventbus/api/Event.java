package net.minecraftforge.eventbus.api;

/**
 * Vibe's clean-room subset of Forge's base event type.
 */
public class Event {
    private boolean cancelable;
    private boolean canceled;
    private Result result = Result.DEFAULT;

    public boolean isCancelable() {
        return cancelable;
    }

    public void setCancelable(boolean cancelable) {
        this.cancelable = cancelable;
    }

    public boolean isCanceled() {
        return canceled;
    }

    public void setCanceled(boolean canceled) {
        if (canceled && !cancelable) {
            throw new UnsupportedOperationException("Attempted to cancel a non-cancelable event");
        }
        this.canceled = canceled;
    }

    public Result getResult() {
        return result;
    }

    public void setResult(Result result) {
        this.result = result;
    }

    public boolean hasResult() {
        return result != Result.DEFAULT;
    }

    public enum Result {
        DENY,
        DEFAULT,
        ALLOW
    }
}
