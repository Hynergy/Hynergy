package dev.hynergy.electrical;

import dev.hynergy.electrical.primitives.controlled.VoltageControlledConductance;
import dev.hynergy.electrical.primitives.controlled.VoltageControlledSwitch;
import dev.hynergy.electrical.primitives.logic.*;
import dev.hynergy.electrical.primitives.passive.*;
import dev.hynergy.electrical.primitives.sources.CurrentSource;
import dev.hynergy.electrical.primitives.sources.VoltageControlledCurrentSource;
import dev.hynergy.electrical.primitives.sources.VoltageControlledVoltageSource;
import dev.hynergy.electrical.primitives.sources.VoltageSource;
import org.junit.jupiter.api.Test;

import java.util.ArrayList;

import static org.junit.jupiter.api.Assertions.*;

final class PrimitiveObservationTest {
    private static final ObservationListener NOOP_LISTENER = (status, value) -> {
    };

    @Test
    void twoTerminalPrimitivesExposeVoltageAndCurrentObservers() {
        try (ElectricalRuntime runtime = ElectricalRuntime.create(); ElectricSystem system = runtime.createSystem(20)) {
            Resistance resistance = system.create(Resistance.TYPE);

            Conductance conductance = system.create(Conductance.TYPE);

            VoltageSource voltageSource = system.create(VoltageSource.TYPE);

            CurrentSource currentSource = system.create(CurrentSource.TYPE);

            Capacitor capacitor = system.create(Capacitor.TYPE);

            Inductor inductor = system.create(Inductor.TYPE);

            Diode diode = system.create(Diode.TYPE);

            assertActive(
                resistance.observeVoltage(NOOP_LISTENER), resistance.observeCurrent(NOOP_LISTENER),

                conductance.observeVoltage(NOOP_LISTENER), conductance.observeCurrent(NOOP_LISTENER),

                voltageSource.observeVoltage(NOOP_LISTENER), voltageSource.observeCurrent(NOOP_LISTENER),

                currentSource.observeVoltage(NOOP_LISTENER), currentSource.observeCurrent(NOOP_LISTENER),

                capacitor.observeVoltage(NOOP_LISTENER), capacitor.observeCurrent(NOOP_LISTENER),

                inductor.observeVoltage(NOOP_LISTENER), inductor.observeCurrent(NOOP_LISTENER),

                diode.observeVoltage(NOOP_LISTENER), diode.observeCurrent(NOOP_LISTENER)
            );
        }
    }

    @Test
    void controlledPrimitivesExposeSemanticObservers() {
        try (ElectricalRuntime runtime = ElectricalRuntime.create(); ElectricSystem system = runtime.createSystem(20)) {
            VoltageControlledCurrentSource currentSource = system.create(VoltageControlledCurrentSource.TYPE);

            VoltageControlledVoltageSource voltageSource = system.create(VoltageControlledVoltageSource.TYPE);

            VoltageControlledSwitch controlledSwitch = system.create(VoltageControlledSwitch.TYPE);

            VoltageControlledConductance conductance = system.create(VoltageControlledConductance.TYPE);

            assertActive(
                currentSource.observeOutputVoltage(NOOP_LISTENER),
                currentSource.observeControlVoltage(NOOP_LISTENER),
                currentSource.observeOutputCurrent(NOOP_LISTENER),

                voltageSource.observeOutputVoltage(NOOP_LISTENER),
                voltageSource.observeControlVoltage(NOOP_LISTENER),
                voltageSource.observeOutputCurrent(NOOP_LISTENER),

                controlledSwitch.observeOutputVoltage(NOOP_LISTENER),
                controlledSwitch.observeControlVoltage(NOOP_LISTENER),
                controlledSwitch.observeOutputCurrent(NOOP_LISTENER),

                conductance.observeOutputVoltage(NOOP_LISTENER),
                conductance.observeControlVoltage(NOOP_LISTENER),
                conductance.observeOutputCurrent(NOOP_LISTENER)
            );
        }
    }

    @Test
    void singleInputLogicPrimitivesExposeSemanticObservers() {
        try (ElectricalRuntime runtime = ElectricalRuntime.create(); ElectricSystem system = runtime.createSystem(20)) {
            Not not = system.create(Not.TYPE);

            SchmittBuffer schmitt = system.create(SchmittBuffer.TYPE);

            assertActive(
                not.observeOutputVoltage(NOOP_LISTENER),
                not.observeInputVoltage(NOOP_LISTENER),
                not.observeOutputCurrent(NOOP_LISTENER),

                schmitt.observeOutputVoltage(NOOP_LISTENER),
                schmitt.observeInputVoltage(NOOP_LISTENER),
                schmitt.observeOutputCurrent(NOOP_LISTENER)
            );
        }
    }

    @Test
    void twoInputLogicPrimitivesExposeSemanticObservers() {
        try (ElectricalRuntime runtime = ElectricalRuntime.create(); ElectricSystem system = runtime.createSystem(20)) {
            And and = system.create(And.TYPE);

            Nand nand = system.create(Nand.TYPE);

            Or or = system.create(Or.TYPE);

            Nor nor = system.create(Nor.TYPE);

            assertActive(
                and.observeOutputVoltage(NOOP_LISTENER),
                and.observeInputVoltageA(NOOP_LISTENER),
                and.observeInputVoltageB(NOOP_LISTENER),
                and.observeOutputCurrent(NOOP_LISTENER),

                nand.observeOutputVoltage(NOOP_LISTENER),
                nand.observeInputVoltageA(NOOP_LISTENER),
                nand.observeInputVoltageB(NOOP_LISTENER),
                nand.observeOutputCurrent(NOOP_LISTENER),

                or.observeOutputVoltage(NOOP_LISTENER),
                or.observeInputVoltageA(NOOP_LISTENER),
                or.observeInputVoltageB(NOOP_LISTENER),
                or.observeOutputCurrent(NOOP_LISTENER),

                nor.observeOutputVoltage(NOOP_LISTENER),
                nor.observeInputVoltageA(NOOP_LISTENER),
                nor.observeInputVoltageB(NOOP_LISTENER),
                nor.observeOutputCurrent(NOOP_LISTENER)
            );
        }
    }

    @Test
    void tickDelayExposesSemanticObservers() {
        try (ElectricalRuntime runtime = ElectricalRuntime.create(); ElectricSystem system = runtime.createSystem(20)) {
            TickDelay delay = system.create(TickDelay.TYPE);

            assertActive(
                delay.observeInputVoltage(NOOP_LISTENER),
                delay.observeOutputVoltage(NOOP_LISTENER),
                delay.observeOutputCurrent(NOOP_LISTENER)
            );
        }
    }

    @Test
    void semanticResistanceObserversPublishExpectedValues() {
        try (ElectricalRuntime runtime = ElectricalRuntime.create(); ElectricSystem system = runtime.createSystem(20)) {
            VoltageSource source = VoltageSource.create(system, 5.0);

            Resistance resistance = Resistance.create(system, 10.0);

            connectSourceAndResistance(system, source, resistance);

            ArrayList<ObservationStatus> voltageStatuses = new ArrayList<>();

            ArrayList<Double> voltages = new ArrayList<>();

            ArrayList<ObservationStatus> currentStatuses = new ArrayList<>();

            ArrayList<Double> currents = new ArrayList<>();

            resistance.observeVoltage((status, value) -> {
                voltageStatuses.add(status);
                voltages.add(value);
            });

            resistance.observeCurrent((status, value) -> {
                currentStatuses.add(status);
                currents.add(value);
            });

            system.tick();

            assertEquals(1, voltageStatuses.size());
            assertEquals(ObservationStatus.AVAILABLE, voltageStatuses.getFirst());
            assertEquals(5.0, voltages.getFirst(), 1e-9);

            assertEquals(1, currentStatuses.size());
            assertEquals(ObservationStatus.AVAILABLE, currentStatuses.getFirst());
            assertEquals(0.5, currents.getFirst(), 1e-9);

            system.tick();

            assertEquals(1, voltages.size());
            assertEquals(1, currents.size());

            source.setVoltage(7.0);

            system.tick();

            assertEquals(2, voltages.size());
            assertEquals(7.0, voltages.get(1), 1e-9);

            assertEquals(2, currents.size());
            assertEquals(0.7, currents.get(1), 1e-9);
        }
    }

    @Test
    void semanticObserverRejectsNullListener() {
        try (ElectricalRuntime runtime = ElectricalRuntime.create(); ElectricSystem system = runtime.createSystem(20)) {
            Resistance resistance = system.create(Resistance.TYPE);

            assertThrows(NullPointerException.class, () -> resistance.observeVoltage(null));
            ObservationSubscription subscription = assertDoesNotThrow(() -> resistance.observeVoltage(NOOP_LISTENER));

            assertTrue(subscription.isActive());
        }
    }

    private static void assertActive(
        ObservationSubscription... subscriptions
    ) {
        for (ObservationSubscription subscription : subscriptions) {
            assertTrue(subscription.isActive());
            assertNotEquals(0, subscription.nativeId());
        }
    }

    private static void connectSourceAndResistance(ElectricSystem system, VoltageSource source, Resistance resistance) {
        Wire positive = system.createWire();
        Wire negative = system.createWire();

        source.attachPositive(positive);
        source.attachNegative(negative);

        resistance.attachPositive(positive);
        resistance.attachNegative(negative);
    }
}