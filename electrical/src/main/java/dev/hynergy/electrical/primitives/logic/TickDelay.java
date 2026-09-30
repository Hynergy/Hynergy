package dev.hynergy.electrical.primitives.logic;

import dev.hynergy.electrical.*;

public final class TickDelay extends Device {
    public static final DeviceType<TickDelay> TYPE = DeviceType.primitive(11, TickDelay::new);

    private static final int TERMINAL_INPUT_POSITIVE = 0;
    private static final int TERMINAL_INPUT_NEGATIVE = 1;
    private static final int TERMINAL_OUTPUT_POSITIVE = 2;
    private static final int TERMINAL_OUTPUT_NEGATIVE = 3;

    private static final int OBSERVER_INPUT_VOLTAGE = 0;
    private static final int OBSERVER_OUTPUT_VOLTAGE = 1;
    private static final int OBSERVER_OUTPUT_CURRENT = 2;

    private TickDelay() {
    }

    public static TickDelay create(ElectricSystem system) {
        return system.create(TYPE);
    }

    public void attachInputPositive(Wire wire) {
        attachTerminal(TERMINAL_INPUT_POSITIVE, wire);
    }

    public void detachInputPositive(Wire wire) {
        detachTerminal(TERMINAL_INPUT_POSITIVE, wire);
    }

    public void attachInputNegative(Wire wire) {
        attachTerminal(TERMINAL_INPUT_NEGATIVE, wire);
    }

    public void detachInputNegative(Wire wire) {
        detachTerminal(TERMINAL_INPUT_NEGATIVE, wire);
    }

    public void attachOutputPositive(Wire wire) {
        attachTerminal(TERMINAL_OUTPUT_POSITIVE, wire);
    }

    public void detachOutputPositive(Wire wire) {
        detachTerminal(TERMINAL_OUTPUT_POSITIVE, wire);
    }

    public void attachOutputNegative(Wire wire) {
        attachTerminal(TERMINAL_OUTPUT_NEGATIVE, wire);
    }

    public void detachOutputNegative(Wire wire) {
        detachTerminal(TERMINAL_OUTPUT_NEGATIVE, wire);
    }


    public ObservationSubscription observeInputVoltage(
        ObservationListener listener
    ) {
        return observe(OBSERVER_INPUT_VOLTAGE, listener);
    }

    public ObservationSubscription observeOutputVoltage(
        ObservationListener listener
    ) {
        return observe(OBSERVER_OUTPUT_VOLTAGE, listener);
    }

    public ObservationSubscription observeOutputCurrent(
        ObservationListener listener
    ) {
        return observe(OBSERVER_OUTPUT_CURRENT, listener);
    }
}
