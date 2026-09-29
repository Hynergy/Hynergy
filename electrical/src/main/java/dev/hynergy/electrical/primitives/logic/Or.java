package dev.hynergy.electrical.primitives.logic;

import dev.hynergy.electrical.Device;
import dev.hynergy.electrical.DeviceType;
import dev.hynergy.electrical.ElectricSystem;
import dev.hynergy.electrical.Wire;

public final class Or extends Device {
    public static final DeviceType<Or> TYPE = DeviceType.primitive(16, Or::new);


    private static final int TERMINAL_A = 0;
    private static final int TERMINAL_B = 1;
    private static final int TERMINAL_C = 2;
    private static final int TERMINAL_D = 3;
    private static final int TERMINAL_E = 4;

    private static final int PARAMETER_VSS_THRESHOLD = 0;
    private static final int PARAMETER_G_MAX = 1;
    private static final int PARAMETER_G_MIN = 2;


    private Or() {
    }

    public static Or create(ElectricSystem system, double vssThreshold, double gMin, double gMax) {
        Or device = system.create(TYPE);

        device.setVssThreshold(vssThreshold);

        device.setGMin(gMin);
        device.setGMax(gMax);

        return device;
    }


    public void setVssThreshold(double value) {
        setParameter(PARAMETER_VSS_THRESHOLD, value);
    }

    public void setGMin(double value) {
        setParameter(PARAMETER_G_MIN, value);
    }

    public void setGMax(double value) {
        setParameter(PARAMETER_G_MAX, value);
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

    public void attachE(Wire wire) {
        attachTerminal(TERMINAL_E, wire);
    }

    public void detachE(Wire wire) {
        detachTerminal(TERMINAL_E, wire);
    }
}
