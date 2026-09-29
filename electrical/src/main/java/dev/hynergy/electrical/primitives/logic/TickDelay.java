package dev.hynergy.electrical.primitives.logic;

import dev.hynergy.electrical.Device;
import dev.hynergy.electrical.DeviceType;
import dev.hynergy.electrical.ElectricSystem;
import dev.hynergy.electrical.Wire;

public final class TickDelay extends Device {
    public static final DeviceType<TickDelay> TYPE = DeviceType.primitive(11, TickDelay::new);

    private static final int TERMINAL_A = 0;
    private static final int TERMINAL_B = 1;
    private static final int TERMINAL_C = 2;
    private static final int TERMINAL_D = 3;


    private TickDelay() {
    }

    public static TickDelay create(ElectricSystem system) {
        return system.create(TYPE);
    }


    public void attachA(Wire wire) {
        attachTerminal(TERMINAL_A, wire);
    }

    public void detachA(Wire wire) {
        detachTerminal(TERMINAL_A, wire);
    }

    public void attachB(Wire wire) {
        attachTerminal(TERMINAL_B, wire);
    }

    public void detachB(Wire wire) {
        detachTerminal(TERMINAL_B, wire);
    }

    public void attachC(Wire wire) {
        attachTerminal(TERMINAL_C, wire);
    }

    public void detachC(Wire wire) {
        detachTerminal(TERMINAL_C, wire);
    }

    public void attachD(Wire wire) {
        attachTerminal(TERMINAL_D, wire);
    }

    public void detachD(Wire wire) {
        detachTerminal(TERMINAL_D, wire);
    }
}
