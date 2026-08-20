package dev.hynergy.mna;

import static org.junit.jupiter.api.Assertions.assertDoesNotThrow;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.lang.foreign.SymbolLookup;
import org.junit.jupiter.api.DisplayName;
import org.junit.jupiter.api.Test;

class HynergyNativeTest {

    @Test
    @DisplayName("the packaged native library loads from the classpath")
    void libraryLoads() {
        SymbolLookup lookup = assertDoesNotThrow(NativeLibrary::lookup);
        assertTrue(lookup.find("hynergy_abi_version").isPresent(),
                "the library loaded but exports no 'hynergy_abi_version' symbol. "
                        + "This is the signature of a missing #[unsafe(no_mangle)] on the Rust "
                        + "side: the .so builds and ships, but its symbol table is empty.");
    }

    @Test
    @DisplayName("calling into Rust returns the expected ABI version")
    void abiVersionMatches() {
        assertEquals(HynergyNative.EXPECTED_ABI_VERSION, HynergyNative.abiVersion());
    }

    @Test
    @DisplayName("the ABI version check accepts the library it was built against")
    void abiVersionCheckPasses() {
        assertDoesNotThrow(HynergyNative::checkAbiVersion);
    }
}
