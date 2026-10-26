//! Assembly's lexical pieces, for x86 (Intel and AT&T) and ARM sources. Mnemonics,
//! registers and size words are case-insensitive. Mnemonics outside the common set, the
//! ARM `@` comment, and AT&T `%` and `$` prefixes, which read as operators or nothing, are
//! not recognized; `bl` and `bx` are the ARM branches, not the x86 registers, and a local
//! label such as `.L1` reads as a directive.

use super::{Marker, Pair, Spec, WordBytes, WordCase};
use crate::syntax::SyntaxClass;

pub static ASM: Spec = Spec {
    line_markers: &[
        Marker::anywhere(";", SyntaxClass::Comment),
        Marker::anywhere("//", SyntaxClass::Comment),
        Marker::at_line_start("#", SyntaxClass::Comment),
    ],
    delimited: &[
        Pair::spanning("/*", "*/", SyntaxClass::Comment),
        Pair::escaped("\"", SyntaxClass::String),
        Pair::escaped("'", SyntaxClass::String),
    ],
    word_bytes: WordBytes::with(b""),
    words: &[
        (
            &[
                "adc", "add", "addl", "addq", "adds", "adr", "adrp", "and", "andl", "andq", "ands",
                "asr", "b", "beq", "bge", "bgt", "bhi", "bic", "bits", "bl", "ble", "blo", "blr",
                "bls", "blt", "bne", "br", "bsf", "bsr", "bswap", "bt", "bx", "call", "callq",
                "cbnz", "cbz", "cdq", "cld", "cmn", "cmp", "cmpl", "cmpq", "cmpxchg", "cpuid",
                "cqo", "csel", "cset", "db", "dd", "dec", "decl", "div", "dq", "dw", "eor", "equ",
                "extern", "global", "hlt", "idiv", "imul", "imull", "imulq", "inc", "incl", "int",
                "ja", "jae", "jb", "jbe", "je", "jg", "jge", "jl", "jle", "jmp", "jmpq", "jne",
                "jns", "jnz", "js", "jz", "ldp", "ldr", "ldrb", "ldrh", "ldrsw", "ldur", "lea",
                "leaq", "leave", "lock", "loop", "lsl", "lsr", "madd", "mov", "movabs", "movaps",
                "movb", "movd", "movk", "movl", "movn", "movq", "movsb", "movsd", "movss", "movsx",
                "movsxd", "movups", "movw", "movz", "movzx", "msub", "mul", "mvn", "neg", "nop",
                "not", "or", "orl", "orq", "orr", "pop", "popq", "push", "pushq", "pxor", "rdtsc",
                "rep", "resb", "resd", "resq", "resw", "ret", "retq", "rol", "ror", "sal", "sar",
                "sbb", "sdiv", "section", "segment", "sete", "setne", "shl", "shlq", "shr", "shrq",
                "smull", "stp", "str", "strb", "strh", "stur", "sub", "subl", "subq", "subs",
                "svc", "syscall", "tbnz", "tbz", "test", "testl", "testq", "times", "tst", "ud2",
                "udiv", "umull", "xchg", "xor", "xorl", "xorq",
            ],
            SyntaxClass::Keyword,
        ),
        (
            &[
                "ah", "al", "ax", "bh", "bp", "bpl", "ch", "cl", "cs", "cx", "dh", "di", "dil",
                "dl", "ds", "dx", "eax", "ebp", "ebx", "ecx", "edi", "edx", "eip", "es", "esi",
                "esp", "fp", "fs", "gs", "ip", "lr", "pc", "r0", "r1", "r10", "r10b", "r10d",
                "r10w", "r11", "r11b", "r11d", "r11w", "r12", "r12b", "r12d", "r12w", "r13",
                "r13b", "r13d", "r13w", "r14", "r14b", "r14d", "r14w", "r15", "r15b", "r15d",
                "r15w", "r2", "r3", "r4", "r5", "r6", "r7", "r8", "r8b", "r8d", "r8w", "r9", "r9b",
                "r9d", "r9w", "rax", "rbp", "rbx", "rcx", "rdi", "rdx", "rip", "rsi", "rsp", "si",
                "sil", "sp", "spl", "ss", "w0", "w1", "w10", "w11", "w12", "w13", "w14", "w15",
                "w16", "w17", "w18", "w19", "w2", "w20", "w21", "w22", "w23", "w24", "w25", "w26",
                "w27", "w28", "w29", "w3", "w30", "w4", "w5", "w6", "w7", "w8", "w9", "wzr", "x0",
                "x1", "x10", "x11", "x12", "x13", "x14", "x15", "x16", "x17", "x18", "x19", "x2",
                "x20", "x21", "x22", "x23", "x24", "x25", "x26", "x27", "x28", "x29", "x3", "x30",
                "x4", "x5", "x6", "x7", "x8", "x9", "xmm0", "xmm1", "xmm10", "xmm11", "xmm12",
                "xmm13", "xmm14", "xmm15", "xmm2", "xmm3", "xmm4", "xmm5", "xmm6", "xmm7", "xmm8",
                "xmm9", "xzr", "ymm0", "ymm1", "ymm10", "ymm11", "ymm12", "ymm13", "ymm14",
                "ymm15", "ymm2", "ymm3", "ymm4", "ymm5", "ymm6", "ymm7", "ymm8", "ymm9",
            ],
            SyntaxClass::Variable,
        ),
        (
            &[
                "byte", "dword", "ptr", "qword", "tword", "word", "xmmword", "ymmword",
            ],
            SyntaxClass::Type,
        ),
    ],
    before: &[(":", SyntaxClass::Function)],
    after: &[
        ("b", SyntaxClass::Function),
        ("bl", SyntaxClass::Function),
        ("call", SyntaxClass::Function),
        ("jmp", SyntaxClass::Function),
        (".", SyntaxClass::Keyword),
    ],
    upper_case: Some(SyntaxClass::Constant),
    capitalized: None,
    other_words: None,
    word_suffixes: b"",
    word_case: WordCase::Insensitive,
};

#[cfg(test)]
mod tests {
    use super::ASM;
    use crate::{scanner::ground, syntax::SyntaxClass};

    #[test]
    fn classifies_a_sample_line() {
        assert_eq!(
            ground(&ASM, "loop_top: add rax, qword ptr [rbx + 8] ; call next"),
            [
                ("loop_top", SyntaxClass::Function),
                ("add", SyntaxClass::Keyword),
                ("rax", SyntaxClass::Variable),
                ("qword", SyntaxClass::Type),
                ("ptr", SyntaxClass::Type),
                ("rbx", SyntaxClass::Variable),
                ("+", SyntaxClass::Operator),
                ("8", SyntaxClass::Number),
                ("; call next", SyntaxClass::Comment),
            ]
        );
    }
}
