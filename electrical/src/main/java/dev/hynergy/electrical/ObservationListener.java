package dev.hynergy.electrical;

@FunctionalInterface
public interface ObservationListener {
    void onUpdate(ObservationStatus status, double value);
}