// SPDX-License-Identifier: MPL-2.0
import java.awt.BorderLayout;
import java.awt.Button;
import java.awt.EventQueue;
import java.awt.FlowLayout;
import java.awt.Frame;
import java.awt.Label;
import java.awt.Panel;
import java.awt.event.WindowAdapter;
import java.awt.event.WindowEvent;
import java.io.IOException;
import java.nio.file.AccessDeniedException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.StandardOpenOption;
import javax.swing.Timer;

/** The only bytecode supplied to the disposable Carbon Java test session. */
final class FixtureSession {
    private static final Path SAVE = Path.of("next-count");
    private static final int MAX_COUNT = 1_000_000;

    public static void main(String[] args) throws Exception {
        if (args.length != 3) throw new IllegalArgumentException("Unexpected fixture inputs");
        Path foreign = Path.of(args[0]);
        Path readOnly = Path.of(args[1]);
        String current = Files.readString(Path.of("initial-count"));
        if (!current.matches("[0-9]{1,7}")) throw new IOException("Invalid sample count");
        int count = Integer.parseInt(current);
        if (count > MAX_COUNT) throw new IOException("Invalid sample count");
        assertDenied(() -> Files.readAllBytes(foreign));
        if (!Files.readString(readOnly).equals("protected fixture"))
            throw new IOException("Read-only fixture unavailable");
        assertDenied(() -> Files.writeString(readOnly, "unexpected write", StandardOpenOption.APPEND));
        EventQueue.invokeAndWait(() -> show(count, args[2].equals("smoke")));
    }

    @FunctionalInterface
    interface IoAction { void run() throws IOException; }

    private static void assertDenied(IoAction access) throws IOException {
        try {
            access.run();
        } catch (AccessDeniedException expected) {
            return;
        }
        throw new IOException("Expected boundary denial");
    }

    private static void show(int initial, boolean smoke) {
        int[] count = {initial};
        Frame frame = new Frame("Carbon Desktop · Protected Java test");
        frame.setLayout(new BorderLayout(8, 10));
        frame.add(new Label("Disposable sample. No Minecraft or account data."), BorderLayout.NORTH);
        Label value = new Label("Sample count: " + initial, Label.CENTER);
        frame.add(value, BorderLayout.CENTER);
        Panel actions = new Panel(new FlowLayout());
        Button add = new Button("Add one");
        Button save = new Button("Save and close");
        Button close = new Button("Close without saving");
        Runnable increment = () -> {
            if (count[0] < MAX_COUNT) count[0]++;
            value.setText("Sample count: " + count[0]);
            add.setEnabled(count[0] < MAX_COUNT);
        };
        Runnable write = () -> {
            try {
                Files.writeString(SAVE, Integer.toString(count[0]), StandardOpenOption.CREATE_NEW);
                frame.dispose();
            } catch (IOException failure) {
                value.setText("Could not write the disposable save.");
            }
        };
        add.addActionListener(event -> increment.run());
        save.addActionListener(event -> write.run());
        close.addActionListener(event -> frame.dispose());
        frame.addWindowListener(new WindowAdapter() {
            @Override public void windowClosing(WindowEvent event) { frame.dispose(); }
        });
        actions.add(add);
        actions.add(save);
        actions.add(close);
        frame.add(actions, BorderLayout.SOUTH);
        frame.setSize(540, 180);
        frame.setLocationByPlatform(true);
        frame.setVisible(true);
        try {
            if (!frame.isShowing() || !frame.isDisplayable())
                throw new IOException("Native window unavailable");
            Files.writeString(Path.of("window-shown"), "checked", StandardOpenOption.CREATE_NEW);
        } catch (IOException failure) {
            frame.dispose();
            throw new IllegalStateException(failure);
        }
        if (smoke) {
            Timer timer = new Timer(850, event -> {
                increment.run();
                write.run();
                frame.dispose();
            });
            timer.setRepeats(false);
            timer.start();
        }
    }
}
