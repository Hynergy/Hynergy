package dev.hynergy.electrical.primitives.passive;

import dev.hynergy.electrical.Device;
import dev.hynergy.electrical.DeviceType;
import dev.hynergy.electrical.ElectricSystem;
import dev.hynergy.electrical.Wire;

public final class Diode extends Device {
    public static final DeviceType<Diode> TYPE = DeviceType.primitive(12, Diode::new);


    private static final int TERMINAL_A = 0;
    private static final int TERMINAL_B = 1;

    private static final int PARAMETER_G_MAX = 0;
    private static final int PARAMETER_G_MIN = 1;


    private Diode() {
    }

    public static Diode create(ElectricSystem system, double gMin, double gMax) {
        Diode device = system.create(TYPE);

        device.setGMin(gMin);
        device.setGMax(gMax);

        return device;
    }


    public void setGMax(double value) {
        setParameter(PARAMETER_G_MAX, value);
    }

    public void setGMin(double value) {
        setParameter(PARAMETER_G_MIN, value);
    }


    public void attachA(Wire wire) {
        attachTerminal(TERMINAL_A, wire);
    }

    public void attachB(Wire wire) {
        attachTerminal(TERMINAL_B, wire);
    }

    public void detachA(Wire wire) {
        detachTerminal(TERMINAL_A, wire);
    }

    public void detachB(Wire wire) {
        detachTerminal(TERMINAL_B, wire);
    }
}
