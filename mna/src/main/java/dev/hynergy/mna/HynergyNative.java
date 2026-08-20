package dev.hynergy.mna;

import java.lang.foreign.FunctionDescriptor;
import java.lang.foreign.Linker;
import java.lang.foreign.MemorySegment;
import java.lang.foreign.SymbolLookup;
import java.lang.foreign.ValueLayout;
import java.lang.invoke.MethodHandle;

/**
 * Bindings to the MNA native engine.
 */
public final class HynergyNative {

    /**
     * ABI version this Java code was written against.
     *
     * <p>Must match {@code ABI_VERSION} in the Rust crate. Raise both together whenever an
     * exported signature changes meaning.
     */
    public static final int EXPECTED_ABI_VERSION = 1;

    private static final Linker LINKER = Linker.nativeLinker();
    private static final SymbolLookup LOOKUP = NativeLibrary.lookup();

    private static final MethodHandle ABI_VERSION =
            downcall("hynergy_abi_version", FunctionDescriptor.of(ValueLayout.JAVA_INT));

    private HynergyNative() {
    }

    /**
     * Returns the ABI version reported by the loaded native library.
     */
    public static int abiVersion() {
        try {
            return (int) ABI_VERSION.invokeExact();
        } catch (Throwable t) {
            throw new IllegalStateException("call to hynergy_abi_version failed", t);
        }
    }

    /**
     * Verifies that the loaded library speaks the same ABI this code expects.
     *
     * @throws IllegalStateException if the versions disagree
     */
    public static void checkAbiVersion() {
        int actual = abiVersion();
        if (actual != EXPECTED_ABI_VERSION) {
            throw new IllegalStateException(
                    "native ABI version mismatch: this build expects " + EXPECTED_ABI_VERSION
                            + " but the loaded library reports " + actual
                            + ". The native library is out of step with the Java code; rebuild it.");
        }
    }

    private static MethodHandle downcall(String symbol, FunctionDescriptor descriptor) {
        MemorySegment address = LOOKUP.find(symbol).orElseThrow(() -> new IllegalStateException(
                "the native library does not export '" + symbol + "'. "
                        + "Check that the Rust function is declared "
                        + "'#[unsafe(no_mangle)] pub extern \"C\"'; without no_mangle the symbol "
                        + "is mangled and cannot be found by name."));
        return LINKER.downcallHandle(address, descriptor);
    }
}
