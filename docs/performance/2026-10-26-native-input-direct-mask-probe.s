	.def	@feat.00;
	.scl	3;
	.type	0;
	.endef
	.globl	@feat.00
@feat.00 = 0
	.att_syntax
	.file	"axis_probe.5fafe5cf027110e8-cgu.0"
	.def	_RNvCs8dl5GLQYRr8_10axis_probe10scale_axis;
	.scl	2;
	.type	32;
	.endef
	.globl	__real@bff0000000000000
	.section	.rdata,"dr",discard,__real@bff0000000000000
	.p2align	3, 0x0
__real@bff0000000000000:
	.quad	0xbff0000000000000
	.globl	__real@40dfffc000000000
	.section	.rdata,"dr",discard,__real@40dfffc000000000
	.p2align	3, 0x0
__real@40dfffc000000000:
	.quad	0x40dfffc000000000
	.globl	__real@c0e0000000000000
	.section	.rdata,"dr",discard,__real@c0e0000000000000
	.p2align	3, 0x0
__real@c0e0000000000000:
	.quad	0xc0e0000000000000
	.section	.text,"xr",one_only,_RNvCs8dl5GLQYRr8_10axis_probe10scale_axis,unique,0
	.globl	_RNvCs8dl5GLQYRr8_10axis_probe10scale_axis
	.p2align	4
_RNvCs8dl5GLQYRr8_10axis_probe10scale_axis:
	movq	%xmm0, %rcx
	shrq	$52, %rcx
	andl	$2047, %ecx
	addl	$3088, %ecx
	andl	$4095, %ecx
	maxsd	__real@bff0000000000000(%rip), %xmm0
	xorl	%edx, %edx
	movsd	__real@40dfffc000000000(%rip), %xmm1
	mulsd	%xmm1, %xmm0
	movsd	__real@c0e0000000000000(%rip), %xmm2
	maxsd	%xmm0, %xmm2
	minsd	%xmm2, %xmm1
	cvttsd2si	%xmm1, %eax
	cmpl	$1039, %ecx
	cmovael	%edx, %eax
	retq

	.def	_RNvCs8dl5GLQYRr8_10axis_probe13scale_trigger;
	.scl	2;
	.type	32;
	.endef
	.section	.text,"xr",one_only,_RNvCs8dl5GLQYRr8_10axis_probe13scale_trigger,unique,1
	.globl	_RNvCs8dl5GLQYRr8_10axis_probe13scale_trigger
	.p2align	4
_RNvCs8dl5GLQYRr8_10axis_probe13scale_trigger:
	movq	%xmm0, %rcx
	shrq	$52, %rcx
	addl	$3088, %ecx
	andl	$4095, %ecx
	xorl	%edx, %edx
	movsd	__real@40dfffc000000000(%rip), %xmm1
	mulsd	%xmm1, %xmm0
	movsd	__real@c0e0000000000000(%rip), %xmm2
	maxsd	%xmm0, %xmm2
	minsd	%xmm2, %xmm1
	cvttsd2si	%xmm1, %eax
	cmpl	$1039, %ecx
	cmovael	%edx, %eax
	retq

	.globl	_fltused
