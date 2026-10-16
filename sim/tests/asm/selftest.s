@ Self-checking ARMv6-M instruction test for the emulator.
@ Ends with `bkpt #0` on success; on failure `bkpt #N`, where N is the check number.
@ Assemble: arm-none-eabi-as -mcpu=cortex-m0 -mthumb selftest.s -o selftest.o
@           arm-none-eabi-ld -Ttext=0 selftest.o -o selftest.elf
@           arm-none-eabi-objcopy -O binary selftest.elf selftest.bin

    .syntax unified
    .cpu cortex-m0
    .thumb

    .macro CHECK reg, value, n
    ldr r7, =\value
    cmp \reg, r7
    beq 1f
    bkpt #\n
1:
    .endm

    .section .text
vectors:
    .word 0x10001FF0
    .word reset + 1
    .word fault + 1         @ NMI
    .word fault + 1         @ HardFault
    .rept 7
    .word fault + 1
    .endr
    .word svc_handler + 1   @ 11 SVC
    .word fault + 1
    .word fault + 1
    .word fault + 1         @ 14 PendSV
    .word systick + 1       @ 15 SysTick

    .thumb_func
fault:
    bkpt #250

    .thumb_func
svc_handler:
    @ Modify the stacked r0 so the caller sees the effect.
    ldr r1, [sp, #0]
    adds r1, #100
    str r1, [sp, #0]
    bx lr

    .thumb_func
systick:
    adds r6, #1
    bx lr

    .thumb_func
reset:
    @ 1-4: add/sub flags
    movs r0, #0
    subs r0, #1             @ 0 - 1 = 0xFFFFFFFF, C=0 (borrow), N=1
    bcc 9f
    bkpt #2
9:
    bmi 9f
    bkpt #2
9:
    CHECK r0, 0xFFFFFFFF, 1
    ldr r0, =0x7FFFFFFF
    adds r0, #1             @ overflow
    bvs 9f
    bkpt #3
9:
    ldr r0, =0xFFFFFFFF
    adds r0, #1             @ carry, zero
    bcs 9f
    bkpt #4
9:
    beq 9f
    bkpt #4
9:

    @ 5-6: adc/sbc
    movs r0, #5
    movs r1, #3
    ldr r2, =0xFFFFFFFF
    adds r2, #1             @ set C
    adcs r0, r1             @ 5+3+1 = 9
    CHECK r0, 9, 5
    movs r0, #5
    movs r2, #0
    cmp r2, #1              @ C=0
    sbcs r0, r1             @ 5-3-1 = 1
    CHECK r0, 1, 6

    @ 7-12: shifts
    movs r0, #1
    movs r1, #32
    lsls r0, r1             @ shift by 32: result 0, C = bit0
    bcs 9f
    bkpt #8
9:
    CHECK r0, 0, 7
    ldr r0, =0x80000000
    asrs r0, r0, #32        @ asr #32 (encoded as 0)
    CHECK r0, 0xFFFFFFFF, 9
    ldr r0, =0x12345678
    movs r1, #8
    rors r0, r1
    CHECK r0, 0x78123456, 10
    ldr r0, =0x80000001
    lsrs r0, r0, #1
    bcs 9f
    bkpt #12
9:

    CHECK r0, 0x40000000, 11

    @ 13-14: multiply, negate
    movs r0, #7
    movs r1, #6
    muls r0, r1
    CHECK r0, 42, 13
    rsbs r0, r0, #0
    CHECK r0, 0xFFFFFFD6, 14

    @ 15-19: extend / reverse
    ldr r0, =0x1234F680
    sxtb r1, r0
    CHECK r1, 0xFFFFFF80, 15
    sxth r1, r0
    CHECK r1, 0xFFFFF680, 16
    uxtb r1, r0
    CHECK r1, 0x80, 17
    rev r1, r0
    CHECK r1, 0x80F63412, 18
    rev16 r1, r0
    CHECK r1, 0x341280F6, 19

    @ 20-24: loads/stores
    ldr r4, =0x10000100
    ldr r0, =0x8899AABB
    str r0, [r4]
    ldrb r1, [r4, #1]
    CHECK r1, 0xAA, 20
    ldrsb r2, [r4, r5]      @ r5 = 0 at reset? use explicit offset below
    movs r5, #3
    ldrsb r2, [r4, r5]
    CHECK r2, 0xFFFFFF88, 21
    movs r5, #2
    ldrsh r2, [r4, r5]
    CHECK r2, 0xFFFF8899, 22
    ldrh r2, [r4, #0]
    CHECK r2, 0xAABB, 23
    movs r0, #0x55
    strb r0, [r4, #2]
    ldr r1, [r4]
    CHECK r1, 0x8855AABB, 24

    @ 25-27: ldm/stm, push/pop
    movs r0, #1
    movs r1, #2
    movs r2, #3
    mov r3, r4
    stm r3!, {r0, r1, r2}
    subs r3, #12
    CHECK r3, 0x10000100, 25
    ldm r3!, {r0, r1, r2}
    adds r0, r1
    adds r0, r2
    CHECK r0, 6, 26
    movs r0, #11
    movs r1, #22
    push {r0, r1}
    pop {r2, r3}
    adds r2, r3
    CHECK r2, 33, 27

    @ 28: bl / bx
    movs r0, #0
    bl sub_add5
    CHECK r0, 5, 28

    @ 29: blx register
    ldr r1, =sub_add5 + 1
    blx r1
    CHECK r0, 10, 29

    @ 30: svc + exception return modifies stacked r0
    movs r0, #1
    svc #0
    CHECK r0, 101, 30

    @ 31-32: primask
    cpsid i
    mrs r0, primask
    CHECK r0, 1, 31
    cpsie i
    mrs r0, primask
    CHECK r0, 0, 32

    @ 33: signed compare / conditional branches
    movs r0, #0
    subs r0, #5              @ -5
    movs r1, #3
    cmp r0, r1
    blt 9f
    bkpt #33
9:

    @ 34: unsigned compare (0xFFFFFFFB > 3)
    cmp r0, r1
    bhi 9f
    bkpt #34
9:

    @ 35: SysTick interrupt fires and returns
    movs r6, #0
    ldr r0, =0xE000E014
    movs r1, #100
    str r1, [r0]            @ RVR = 100
    ldr r0, =0xE000E010
    movs r1, #7
    str r1, [r0]            @ enable, tickint, core clock
    ldr r2, =2000
3:
    subs r2, #1
    bne 3b
    movs r1, #0
    str r1, [r0]
    cmp r6, #5
    bhs 9f
    bkpt #35
9:

    bkpt #0

    .thumb_func
sub_add5:
    adds r0, #5
    bx lr


    .ltorg
