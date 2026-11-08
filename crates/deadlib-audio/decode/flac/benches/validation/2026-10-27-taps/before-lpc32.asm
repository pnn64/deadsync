
/tmp/deadlib-flac-decode-pipelines-candidate:     file format elf64-x86-64


Disassembly of section .text:

000000000004c490 <_RINvNtNtCs6l4HYnHGbZv_25deadlib_audio_decode_flac9predictor3x866kernelKj20_EB6_>:
   4c490:	55                   	push   %rbp
   4c491:	48 89 e5             	mov    %rsp,%rbp
   4c494:	41 57                	push   %r15
   4c496:	41 56                	push   %r14
   4c498:	41 55                	push   %r13
   4c49a:	41 54                	push   %r12
   4c49c:	53                   	push   %rbx
   4c49d:	48 83 e4 e0          	and    $0xffffffffffffffe0,%rsp
   4c4a1:	48 81 ec 60 01 00 00 	sub    $0x160,%rsp
   4c4a8:	49 89 ca             	mov    %rcx,%r10
   4c4ab:	48 89 54 24 10       	mov    %rdx,0x10(%rsp)
   4c4b0:	48 89 7c 24 08       	mov    %rdi,0x8(%rsp)
   4c4b5:	41 8d 49 ff          	lea    -0x1(%r9),%ecx
   4c4b9:	41 bb 01 00 00 00    	mov    $0x1,%r11d
   4c4bf:	49 d3 e3             	shl    %cl,%r11
   4c4c2:	44 89 c0             	mov    %r8d,%eax
   4c4c5:	44 89 c9             	mov    %r9d,%ecx
   4c4c8:	48 89 4c 24 18       	mov    %rcx,0x18(%rsp)
   4c4cd:	c4 c1 7a 6f 02       	vmovdqu (%r10),%xmm0
   4c4d2:	4d 8b 32             	mov    (%r10),%r14
   4c4d5:	4d 8b 7a 08          	mov    0x8(%r10),%r15
   4c4d9:	49 8b 4a 10          	mov    0x10(%r10),%rcx
   4c4dd:	48 89 4c 24 20       	mov    %rcx,0x20(%rsp)
   4c4e2:	4c 8d a6 f8 00 00 00 	lea    0xf8(%rsi),%r12
   4c4e9:	41 b9 20 00 00 00    	mov    $0x20,%r9d
   4c4ef:	c5 f1 ef c9          	vpxor  %xmm1,%xmm1,%xmm1
   4c4f3:	66 66 66 66 2e 0f 1f 	data16 data16 data16 cs nopw 0x0(%rax,%rax,1)
   4c4fa:	84 00 00 00 00 00
   4c500:	c5 fd 7f 8c 24 a0 00 	vmovdqa %ymm1,0xa0(%rsp)
   4c507:	00 00
   4c509:	c5 fd 7f 8c 24 80 00 	vmovdqa %ymm1,0x80(%rsp)
   4c510:	00 00
   4c512:	c5 fd 7f 4c 24 60    	vmovdqa %ymm1,0x60(%rsp)
   4c518:	c5 fd 7f 4c 24 40    	vmovdqa %ymm1,0x40(%rsp)
   4c51e:	4c 89 e1             	mov    %r12,%rcx
   4c521:	31 d2                	xor    %edx,%edx
   4c523:	66 66 66 66 2e 0f 1f 	data16 data16 data16 cs nopw 0x0(%rax,%rax,1)
   4c52a:	84 00 00 00 00 00
   4c530:	48 8d 7a 01          	lea    0x1(%rdx),%rdi
   4c534:	4c 63 01             	movslq (%rcx),%r8
   4c537:	c4 c1 f9 6e d0       	vmovq  %r8,%xmm2
   4c53c:	c4 e2 7d 59 d2       	vpbroadcastq %xmm2,%ymm2
   4c541:	c4 c2 6d 28 14 d2    	vpmuldq (%r10,%rdx,8),%ymm2,%ymm2
   4c547:	83 e2 03             	and    $0x3,%edx
   4c54a:	c1 e2 05             	shl    $0x5,%edx
   4c54d:	c5 ed d4 54 14 40    	vpaddq 0x40(%rsp,%rdx,1),%ymm2,%ymm2
   4c553:	c5 fd 7f 54 14 40    	vmovdqa %ymm2,0x40(%rsp,%rdx,1)
   4c559:	48 83 c1 f8          	add    $0xfffffffffffffff8,%rcx
   4c55d:	48 89 fa             	mov    %rdi,%rdx
   4c560:	48 83 ff 20          	cmp    $0x20,%rdi
   4c564:	75 ca                	jne    4c530 <_RINvNtNtCs6l4HYnHGbZv_25deadlib_audio_decode_flac9predictor3x866kernelKj20_EB6_+0xa0>
   4c566:	c5 fd 6f 54 24 60    	vmovdqa 0x60(%rsp),%ymm2
   4c56c:	c5 ed d4 54 24 40    	vpaddq 0x40(%rsp),%ymm2,%ymm2
   4c572:	c5 ed d4 94 24 80 00 	vpaddq 0x80(%rsp),%ymm2,%ymm2
   4c579:	00 00
   4c57b:	c5 ed d4 94 24 a0 00 	vpaddq 0xa0(%rsp),%ymm2,%ymm2
   4c582:	00 00
   4c584:	c4 e1 f9 7e d7       	vmovq  %xmm2,%rdi
   4c589:	89 c1                	mov    %eax,%ecx
   4c58b:	48 d3 ff             	sar    %cl,%rdi
   4c58e:	4a 03 3c ce          	add    (%rsi,%r9,8),%rdi
   4c592:	4e 8d 04 1f          	lea    (%rdi,%r11,1),%r8
   4c596:	c4 e3 f9 16 d1 01    	vpextrq $0x1,%xmm2,%rcx
   4c59c:	4a 89 3c ce          	mov    %rdi,(%rsi,%r9,8)
   4c5a0:	48 89 fa             	mov    %rdi,%rdx
   4c5a3:	49 0f af d6          	imul   %r14,%rdx
   4c5a7:	48 01 ca             	add    %rcx,%rdx
   4c5aa:	89 c1                	mov    %eax,%ecx
   4c5ac:	48 d3 fa             	sar    %cl,%rdx
   4c5af:	4a 03 54 ce 08       	add    0x8(%rsi,%r9,8),%rdx
   4c5b4:	c4 e3 7d 39 d2 01    	vextracti128 $0x1,%ymm2,%xmm2
   4c5ba:	c4 e1 f9 7e d1       	vmovq  %xmm2,%rcx
   4c5bf:	49 89 fd             	mov    %rdi,%r13
   4c5c2:	4d 0f af ef          	imul   %r15,%r13
   4c5c6:	49 01 cd             	add    %rcx,%r13
   4c5c9:	48 89 d1             	mov    %rdx,%rcx
   4c5cc:	49 0f af ce          	imul   %r14,%rcx
   4c5d0:	49 01 cd             	add    %rcx,%r13
   4c5d3:	c4 e3 f9 16 d1 01    	vpextrq $0x1,%xmm2,%rcx
   4c5d9:	48 0f af 7c 24 20    	imul   0x20(%rsp),%rdi
   4c5df:	48 01 cf             	add    %rcx,%rdi
   4c5e2:	4a 8d 1c 1a          	lea    (%rdx,%r11,1),%rbx
   4c5e6:	4a 89 54 ce 08       	mov    %rdx,0x8(%rsi,%r9,8)
   4c5eb:	89 c1                	mov    %eax,%ecx
   4c5ed:	49 d3 fd             	sar    %cl,%r13
   4c5f0:	4e 03 6c ce 10       	add    0x10(%rsi,%r9,8),%r13
   4c5f5:	49 0f af d7          	imul   %r15,%rdx
   4c5f9:	48 01 d7             	add    %rdx,%rdi
   4c5fc:	4b 8d 14 2b          	lea    (%r11,%r13,1),%rdx
   4c600:	4e 89 6c ce 10       	mov    %r13,0x10(%rsi,%r9,8)
   4c605:	4d 0f af ee          	imul   %r14,%r13
   4c609:	4c 01 ef             	add    %r13,%rdi
   4c60c:	48 d3 ff             	sar    %cl,%rdi
   4c60f:	4c 09 c3             	or     %r8,%rbx
   4c612:	48 09 da             	or     %rbx,%rdx
   4c615:	4a 03 7c ce 18       	add    0x18(%rsi,%r9,8),%rdi
   4c61a:	4e 8d 04 1f          	lea    (%rdi,%r11,1),%r8
   4c61e:	49 09 d0             	or     %rdx,%r8
   4c621:	4a 89 7c ce 18       	mov    %rdi,0x18(%rsi,%r9,8)
   4c626:	48 8b 4c 24 18       	mov    0x18(%rsp),%rcx
   4c62b:	49 d3 e8             	shr    %cl,%r8
   4c62e:	4d 85 c0             	test   %r8,%r8
   4c631:	0f 85 b3 02 00 00    	jne    4c8ea <_RINvNtNtCs6l4HYnHGbZv_25deadlib_audio_decode_flac9predictor3x866kernelKj20_EB6_+0x45a>
   4c637:	49 83 c1 04          	add    $0x4,%r9
   4c63b:	48 8b 4c 24 10       	mov    0x10(%rsp),%rcx
   4c640:	4c 29 c9             	sub    %r9,%rcx
   4c643:	49 83 c4 20          	add    $0x20,%r12
   4c647:	48 83 f9 03          	cmp    $0x3,%rcx
   4c64b:	0f 87 af fe ff ff    	ja     4c500 <_RINvNtNtCs6l4HYnHGbZv_25deadlib_audio_decode_flac9predictor3x866kernelKj20_EB6_+0x70>
   4c651:	4c 39 4c 24 10       	cmp    %r9,0x10(%rsp)
   4c656:	0f 86 80 02 00 00    	jbe    4c8dc <_RINvNtNtCs6l4HYnHGbZv_25deadlib_audio_decode_flac9predictor3x866kernelKj20_EB6_+0x44c>
   4c65c:	c4 e3 fd 00 f8 14    	vpermq $0x14,%ymm0,%ymm7
   4c662:	c4 c1 7a 7e 42 18    	vmovq  0x18(%r10),%xmm0
   4c668:	c4 63 f9 22 44 24 20 	vpinsrq $0x1,0x20(%rsp),%xmm0,%xmm8
   4c66f:	01
   4c670:	c4 c3 fd 00 42 20 1b 	vpermq $0x1b,0x20(%r10),%ymm0
   4c677:	c4 c3 fd 00 4a 40 1b 	vpermq $0x1b,0x40(%r10),%ymm1
   4c67e:	c4 c3 fd 00 52 60 1b 	vpermq $0x1b,0x60(%r10),%ymm2
   4c685:	c4 c3 fd 00 9a 80 00 	vpermq $0x1b,0x80(%r10),%ymm3
   4c68c:	00 00 1b
   4c68f:	c4 c3 fd 00 a2 a0 00 	vpermq $0x1b,0xa0(%r10),%ymm4
   4c696:	00 00 1b
   4c699:	c4 c3 fd 00 aa c0 00 	vpermq $0x1b,0xc0(%r10),%ymm5
   4c6a0:	00 00 1b
   4c6a3:	c4 c3 fd 00 b2 e0 00 	vpermq $0x1b,0xe0(%r10),%ymm6
   4c6aa:	00 00 1b
   4c6ad:	c4 c3 45 02 f8 0f    	vpblendd $0xf,%ymm8,%ymm7,%ymm7
   4c6b3:	c5 bd 73 d4 20       	vpsrlq $0x20,%ymm4,%ymm8
   4c6b8:	c5 7d 7f 44 24 20    	vmovdqa %ymm8,0x20(%rsp)
   4c6be:	c5 bd 73 d0 20       	vpsrlq $0x20,%ymm0,%ymm8
   4c6c3:	c5 7d 7f 84 24 20 01 	vmovdqa %ymm8,0x120(%rsp)
   4c6ca:	00 00
   4c6cc:	c5 bd 73 d6 20       	vpsrlq $0x20,%ymm6,%ymm8
   4c6d1:	c5 7d 7f 84 24 00 01 	vmovdqa %ymm8,0x100(%rsp)
   4c6d8:	00 00
   4c6da:	c5 bd 73 d2 20       	vpsrlq $0x20,%ymm2,%ymm8
   4c6df:	c5 7d 7f 84 24 e0 00 	vmovdqa %ymm8,0xe0(%rsp)
   4c6e6:	00 00
   4c6e8:	c5 bd 73 d3 20       	vpsrlq $0x20,%ymm3,%ymm8
   4c6ed:	c5 7d 7f 84 24 c0 00 	vmovdqa %ymm8,0xc0(%rsp)
   4c6f4:	00 00
   4c6f6:	c5 95 73 d7 20       	vpsrlq $0x20,%ymm7,%ymm13
   4c6fb:	c5 8d 73 d5 20       	vpsrlq $0x20,%ymm5,%ymm14
   4c700:	c4 21 7e 6f 7c ce a0 	vmovdqu -0x60(%rsi,%r9,8),%ymm15
   4c707:	c4 21 7e 6f 44 ce 80 	vmovdqu -0x80(%rsi,%r9,8),%ymm8
   4c70e:	c4 21 7e 6f 4c ce c0 	vmovdqu -0x40(%rsi,%r9,8),%ymm9
   4c715:	c4 21 7e 6f 94 ce 40 	vmovdqu -0xc0(%rsi,%r9,8),%ymm10
   4c71c:	ff ff ff
   4c71f:	c4 c1 25 73 d2 20    	vpsrlq $0x20,%ymm10,%ymm11
   4c725:	c5 25 f4 dc          	vpmuludq %ymm4,%ymm11,%ymm11
   4c729:	c5 2d f4 64 24 20    	vpmuludq 0x20(%rsp),%ymm10,%ymm12
   4c72f:	c4 41 1d d4 db       	vpaddq %ymm11,%ymm12,%ymm11
   4c734:	c4 c1 25 73 f3 20    	vpsllq $0x20,%ymm11,%ymm11
   4c73a:	c5 2d f4 d4          	vpmuludq %ymm4,%ymm10,%ymm10
   4c73e:	c4 41 2d d4 d3       	vpaddq %ymm11,%ymm10,%ymm10
   4c743:	c4 c1 25 73 d1 20    	vpsrlq $0x20,%ymm9,%ymm11
   4c749:	c5 25 f4 d8          	vpmuludq %ymm0,%ymm11,%ymm11
   4c74d:	c5 35 f4 a4 24 20 01 	vpmuludq 0x120(%rsp),%ymm9,%ymm12
   4c754:	00 00
   4c756:	c4 41 1d d4 db       	vpaddq %ymm11,%ymm12,%ymm11
   4c75b:	c4 21 7e 6f a4 ce 00 	vmovdqu -0x100(%rsi,%r9,8),%ymm12
   4c762:	ff ff ff
   4c765:	c4 c1 25 73 f3 20    	vpsllq $0x20,%ymm11,%ymm11
   4c76b:	c5 35 f4 c8          	vpmuludq %ymm0,%ymm9,%ymm9
   4c76f:	c4 41 35 d4 cb       	vpaddq %ymm11,%ymm9,%ymm9
   4c774:	c4 41 2d d4 c9       	vpaddq %ymm9,%ymm10,%ymm9
   4c779:	c5 1d f4 94 24 00 01 	vpmuludq 0x100(%rsp),%ymm12,%ymm10
   4c780:	00 00
   4c782:	c4 c1 25 73 d4 20    	vpsrlq $0x20,%ymm12,%ymm11
   4c788:	c5 25 f4 de          	vpmuludq %ymm6,%ymm11,%ymm11
   4c78c:	c4 41 2d d4 d3       	vpaddq %ymm11,%ymm10,%ymm10
   4c791:	c4 c1 2d 73 f2 20    	vpsllq $0x20,%ymm10,%ymm10
   4c797:	c5 1d f4 de          	vpmuludq %ymm6,%ymm12,%ymm11
   4c79b:	c4 41 25 d4 d2       	vpaddq %ymm10,%ymm11,%ymm10
   4c7a0:	c4 c1 25 73 d0 20    	vpsrlq $0x20,%ymm8,%ymm11
   4c7a6:	c5 25 f4 da          	vpmuludq %ymm2,%ymm11,%ymm11
   4c7aa:	c5 3d f4 a4 24 e0 00 	vpmuludq 0xe0(%rsp),%ymm8,%ymm12
   4c7b1:	00 00
   4c7b3:	c4 41 1d d4 db       	vpaddq %ymm11,%ymm12,%ymm11
   4c7b8:	c4 21 7e 6f 64 ce e0 	vmovdqu -0x20(%rsi,%r9,8),%ymm12
   4c7bf:	c4 c1 25 73 f3 20    	vpsllq $0x20,%ymm11,%ymm11
   4c7c5:	c5 3d f4 c2          	vpmuludq %ymm2,%ymm8,%ymm8
   4c7c9:	c4 41 3d d4 c3       	vpaddq %ymm11,%ymm8,%ymm8
   4c7ce:	c4 21 7e 6f 9c ce 60 	vmovdqu -0xa0(%rsi,%r9,8),%ymm11
   4c7d5:	ff ff ff
   4c7d8:	c4 41 2d d4 c0       	vpaddq %ymm8,%ymm10,%ymm8
   4c7dd:	c4 41 3d d4 c1       	vpaddq %ymm9,%ymm8,%ymm8
   4c7e2:	c4 c1 35 73 d3 20    	vpsrlq $0x20,%ymm11,%ymm9
   4c7e8:	c5 35 f4 cb          	vpmuludq %ymm3,%ymm9,%ymm9
   4c7ec:	c5 25 f4 94 24 c0 00 	vpmuludq 0xc0(%rsp),%ymm11,%ymm10
   4c7f3:	00 00
   4c7f5:	c4 41 2d d4 c9       	vpaddq %ymm9,%ymm10,%ymm9
   4c7fa:	c4 c1 35 73 f1 20    	vpsllq $0x20,%ymm9,%ymm9
   4c800:	c5 25 f4 d3          	vpmuludq %ymm3,%ymm11,%ymm10
   4c804:	c4 41 2d d4 c9       	vpaddq %ymm9,%ymm10,%ymm9
   4c809:	c4 c1 2d 73 d4 20    	vpsrlq $0x20,%ymm12,%ymm10
   4c80f:	c5 2d f4 d7          	vpmuludq %ymm7,%ymm10,%ymm10
   4c813:	c4 41 1d f4 dd       	vpmuludq %ymm13,%ymm12,%ymm11
   4c818:	c4 41 25 d4 d2       	vpaddq %ymm10,%ymm11,%ymm10
   4c81d:	c4 21 7e 6f 9c ce 20 	vmovdqu -0xe0(%rsi,%r9,8),%ymm11
   4c824:	ff ff ff
   4c827:	c4 c1 2d 73 f2 20    	vpsllq $0x20,%ymm10,%ymm10
   4c82d:	c5 1d f4 e7          	vpmuludq %ymm7,%ymm12,%ymm12
   4c831:	c4 41 1d d4 d2       	vpaddq %ymm10,%ymm12,%ymm10
   4c836:	c4 41 35 d4 ca       	vpaddq %ymm10,%ymm9,%ymm9
   4c83b:	c4 c1 2d 73 d3 20    	vpsrlq $0x20,%ymm11,%ymm10
   4c841:	c5 2d f4 d5          	vpmuludq %ymm5,%ymm10,%ymm10
   4c845:	c4 41 25 f4 e6       	vpmuludq %ymm14,%ymm11,%ymm12
   4c84a:	c4 41 1d d4 d2       	vpaddq %ymm10,%ymm12,%ymm10
   4c84f:	c4 c1 2d 73 f2 20    	vpsllq $0x20,%ymm10,%ymm10
   4c855:	c5 25 f4 dd          	vpmuludq %ymm5,%ymm11,%ymm11
   4c859:	c4 41 25 d4 d2       	vpaddq %ymm10,%ymm11,%ymm10
   4c85e:	c4 c1 25 73 d7 20    	vpsrlq $0x20,%ymm15,%ymm11
   4c864:	c5 25 f4 d9          	vpmuludq %ymm1,%ymm11,%ymm11
   4c868:	c5 9d 73 d1 20       	vpsrlq $0x20,%ymm1,%ymm12
   4c86d:	c4 41 05 f4 e4       	vpmuludq %ymm12,%ymm15,%ymm12
   4c872:	c4 41 1d d4 db       	vpaddq %ymm11,%ymm12,%ymm11
   4c877:	c4 c1 25 73 f3 20    	vpsllq $0x20,%ymm11,%ymm11
   4c87d:	c5 05 f4 e1          	vpmuludq %ymm1,%ymm15,%ymm12
   4c881:	c4 41 1d d4 db       	vpaddq %ymm11,%ymm12,%ymm11
   4c886:	c4 41 2d d4 d3       	vpaddq %ymm11,%ymm10,%ymm10
   4c88b:	c4 41 2d d4 c9       	vpaddq %ymm9,%ymm10,%ymm9
   4c890:	c4 41 3d d4 c1       	vpaddq %ymm9,%ymm8,%ymm8
   4c895:	c4 43 7d 39 c1 01    	vextracti128 $0x1,%ymm8,%xmm9
   4c89b:	c4 41 39 d4 c1       	vpaddq %xmm9,%xmm8,%xmm8
   4c8a0:	c4 41 79 70 c8 ee    	vpshufd $0xee,%xmm8,%xmm9
   4c8a6:	c4 41 39 d4 c1       	vpaddq %xmm9,%xmm8,%xmm8
   4c8ab:	c4 61 f9 7e c2       	vmovq  %xmm8,%rdx
   4c8b0:	89 c1                	mov    %eax,%ecx
   4c8b2:	48 d3 fa             	sar    %cl,%rdx
   4c8b5:	4a 03 14 ce          	add    (%rsi,%r9,8),%rdx
   4c8b9:	4a 8d 3c 1a          	lea    (%rdx,%r11,1),%rdi
   4c8bd:	48 8b 4c 24 18       	mov    0x18(%rsp),%rcx
   4c8c2:	48 d3 ef             	shr    %cl,%rdi
   4c8c5:	48 85 ff             	test   %rdi,%rdi
   4c8c8:	75 20                	jne    4c8ea <_RINvNtNtCs6l4HYnHGbZv_25deadlib_audio_decode_flac9predictor3x866kernelKj20_EB6_+0x45a>
   4c8ca:	4a 89 14 ce          	mov    %rdx,(%rsi,%r9,8)
   4c8ce:	49 ff c1             	inc    %r9
   4c8d1:	4c 39 4c 24 10       	cmp    %r9,0x10(%rsp)
   4c8d6:	0f 85 24 fe ff ff    	jne    4c700 <_RINvNtNtCs6l4HYnHGbZv_25deadlib_audio_decode_flac9predictor3x866kernelKj20_EB6_+0x270>
   4c8dc:	48 8b 44 24 08       	mov    0x8(%rsp),%rax
   4c8e1:	48 c7 00 ff ff ff ff 	movq   $0xffffffffffffffff,(%rax)
   4c8e8:	eb 1f                	jmp    4c909 <_RINvNtNtCs6l4HYnHGbZv_25deadlib_audio_decode_flac9predictor3x866kernelKj20_EB6_+0x479>
   4c8ea:	48 8b 4c 24 08       	mov    0x8(%rsp),%rcx
   4c8ef:	48 c7 01 01 00 00 00 	movq   $0x1,(%rcx)
   4c8f6:	48 8d 05 cd 1a fc ff 	lea    -0x3e533(%rip),%rax        # e3ca <_RNvNtNtNtCskADBYZwJmSU_4core7unicode12unicode_data2cf17SHORT_OFFSET_RUNS+0xaba>
   4c8fd:	48 89 41 08          	mov    %rax,0x8(%rcx)
   4c901:	48 c7 41 10 21 00 00 	movq   $0x21,0x10(%rcx)
   4c908:	00
   4c909:	48 8d 65 d8          	lea    -0x28(%rbp),%rsp
   4c90d:	5b                   	pop    %rbx
   4c90e:	41 5c                	pop    %r12
   4c910:	41 5d                	pop    %r13
   4c912:	41 5e                	pop    %r14
   4c914:	41 5f                	pop    %r15
   4c916:	5d                   	pop    %rbp
   4c917:	c5 f8 77             	vzeroupper
   4c91a:	c3                   	ret

Disassembly of section .init:

Disassembly of section .fini:

Disassembly of section .plt:
