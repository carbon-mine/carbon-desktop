import java.awt.EventQueue;
import java.awt.Frame;
import java.io.IOException;
import java.nio.file.AccessDeniedException;
import java.nio.file.Files;
import java.nio.file.Path;

/** Benign development fixture. Runs wholly under the same AppContainer as its parent. */
class FixtureProcessor {
    public static void main(String[] args) throws Exception {
        if (!Files.readString(Path.of("private-input")).equals("private")) {
            throw new IOException("Selected input is unavailable");
        }
        try {
            Files.readString(Path.of(args[0], "private-input"));
            throw new IOException("Sibling input was readable");
        } catch (AccessDeniedException expected) {
            // The host created the input before launch.
        }
        Files.writeString(Path.of("processor-result"), "checked");
        EventQueue.invokeAndWait(() -> {
            Frame frame = new Frame("Carbon Desktop · Java window probe");
            try {
                frame.setSize(320, 160);
                frame.setVisible(true);
                if (!frame.isShowing() || !frame.isDisplayable()) {
                    throw new IllegalStateException("Native peer unavailable");
                }
            } finally {
                frame.dispose();
            }
        });
        Files.writeString(Path.of("window-result"), "checked");
    }
}
