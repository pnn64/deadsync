
/tmp/deadlib-flac-decode-unrolled-taps:     file format elf64-x86-64


Disassembly of section .text:

00000000000480c0 <_RINvNtNtCs6l4HYnHGbZv_25deadlib_audio_decode_flac9predictor3x866kernelKj20_EB6_>:
   480c0:	55                   	push   %rbp
   480c1:	41 57                	push   %r15
   480c3:	41 56                	push   %r14
   480c5:	41 55                	push   %r13
   480c7:	41 54                	push   %r12
   480c9:	53                   	push   %rbx
   480ca:	48 81 ec 08 02 00 00 	sub    $0x208,%rsp
   480d1:	49 89 ca             	mov    %rcx,%r10
   480d4:	41 8d 49 ff          	lea    -0x1(%r9),%ecx
   480d8:	41 bb 01 00 00 00    	mov    $0x1,%r11d
   480de:	49 d3 e3             	shl    %cl,%r11
   480e1:	48 89 7c 24 80       	mov    %rdi,-0x80(%rsp)
   480e6:	c4 c1 7c 10 02       	vmovups (%r10),%ymm0
   480eb:	c5 fc 11 44 24 20    	vmovups %ymm0,0x20(%rsp)
   480f1:	c4 c1 7c 10 42 08    	vmovups 0x8(%r10),%ymm0
   480f7:	c5 fc 11 04 24       	vmovups %ymm0,(%rsp)
   480fc:	c4 c1 7c 10 42 10    	vmovups 0x10(%r10),%ymm0
   48102:	c5 fc 11 44 24 e0    	vmovups %ymm0,-0x20(%rsp)
   48108:	c4 c1 7c 10 42 18    	vmovups 0x18(%r10),%ymm0
   4810e:	c5 fc 11 44 24 c0    	vmovups %ymm0,-0x40(%rsp)
   48114:	c4 c1 7c 10 42 20    	vmovups 0x20(%r10),%ymm0
   4811a:	c5 fc 11 44 24 a0    	vmovups %ymm0,-0x60(%rsp)
   48120:	c4 c1 7c 10 42 28    	vmovups 0x28(%r10),%ymm0
   48126:	c5 fc 11 84 24 e0 01 	vmovups %ymm0,0x1e0(%rsp)
   4812d:	00 00
   4812f:	c4 c1 7c 10 42 30    	vmovups 0x30(%r10),%ymm0
   48135:	c5 fc 11 84 24 c0 01 	vmovups %ymm0,0x1c0(%rsp)
   4813c:	00 00
   4813e:	c4 c1 7c 10 42 38    	vmovups 0x38(%r10),%ymm0
   48144:	c5 fc 11 84 24 a0 01 	vmovups %ymm0,0x1a0(%rsp)
   4814b:	00 00
   4814d:	c4 c1 7c 10 42 40    	vmovups 0x40(%r10),%ymm0
   48153:	c5 fc 11 84 24 80 01 	vmovups %ymm0,0x180(%rsp)
   4815a:	00 00
   4815c:	c4 c1 7c 10 42 48    	vmovups 0x48(%r10),%ymm0
   48162:	c5 fc 11 84 24 60 01 	vmovups %ymm0,0x160(%rsp)
   48169:	00 00
   4816b:	c4 c1 7c 10 42 50    	vmovups 0x50(%r10),%ymm0
   48171:	c5 fc 11 84 24 40 01 	vmovups %ymm0,0x140(%rsp)
   48178:	00 00
   4817a:	c4 c1 7c 10 42 58    	vmovups 0x58(%r10),%ymm0
   48180:	c5 fc 11 84 24 20 01 	vmovups %ymm0,0x120(%rsp)
   48187:	00 00
   48189:	c4 c1 7c 10 42 60    	vmovups 0x60(%r10),%ymm0
   4818f:	c5 fc 11 84 24 00 01 	vmovups %ymm0,0x100(%rsp)
   48196:	00 00
   48198:	c4 c1 7c 10 42 68    	vmovups 0x68(%r10),%ymm0
   4819e:	c5 fc 11 84 24 e0 00 	vmovups %ymm0,0xe0(%rsp)
   481a5:	00 00
   481a7:	c4 c1 7c 10 42 70    	vmovups 0x70(%r10),%ymm0
   481ad:	c5 fc 11 84 24 c0 00 	vmovups %ymm0,0xc0(%rsp)
   481b4:	00 00
   481b6:	c4 c1 7c 10 42 78    	vmovups 0x78(%r10),%ymm0
   481bc:	c5 fc 11 84 24 a0 00 	vmovups %ymm0,0xa0(%rsp)
   481c3:	00 00
   481c5:	44 89 c0             	mov    %r8d,%eax
   481c8:	44 89 c9             	mov    %r9d,%ecx
   481cb:	48 89 4c 24 88       	mov    %rcx,-0x78(%rsp)
   481d0:	49 8b 1a             	mov    (%r10),%rbx
   481d3:	4d 8b 72 08          	mov    0x8(%r10),%r14
   481d7:	49 8b 4a 10          	mov    0x10(%r10),%rcx
   481db:	48 89 4c 24 98       	mov    %rcx,-0x68(%rsp)
   481e0:	48 89 54 24 90       	mov    %rdx,-0x70(%rsp)
   481e5:	4c 8d 62 e0          	lea    -0x20(%rdx),%r12
   481e9:	41 b9 20 00 00 00    	mov    $0x20,%r9d
   481ef:	c4 c1 7c 10 82 80 00 	vmovups 0x80(%r10),%ymm0
   481f6:	00 00
   481f8:	c5 fc 11 84 24 80 00 	vmovups %ymm0,0x80(%rsp)
   481ff:	00 00
   48201:	c4 c1 7c 10 82 88 00 	vmovups 0x88(%r10),%ymm0
   48208:	00 00
   4820a:	c5 fc 11 44 24 60    	vmovups %ymm0,0x60(%rsp)
   48210:	c4 c1 7e 6f 82 90 00 	vmovdqu 0x90(%r10),%ymm0
   48217:	00 00
   48219:	c5 fe 7f 44 24 40    	vmovdqu %ymm0,0x40(%rsp)
   4821f:	c4 c1 7e 6f 9a 98 00 	vmovdqu 0x98(%r10),%ymm3
   48226:	00 00
   48228:	c4 c1 7e 6f a2 a0 00 	vmovdqu 0xa0(%r10),%ymm4
   4822f:	00 00
   48231:	c4 c1 7e 6f aa a8 00 	vmovdqu 0xa8(%r10),%ymm5
   48238:	00 00
   4823a:	c4 c1 7e 6f b2 b0 00 	vmovdqu 0xb0(%r10),%ymm6
   48241:	00 00
   48243:	c4 c1 7e 6f ba b8 00 	vmovdqu 0xb8(%r10),%ymm7
   4824a:	00 00
   4824c:	c4 41 7e 6f 82 c0 00 	vmovdqu 0xc0(%r10),%ymm8
   48253:	00 00
   48255:	c4 41 7e 6f 8a c8 00 	vmovdqu 0xc8(%r10),%ymm9
   4825c:	00 00
   4825e:	c4 41 7e 6f 92 d0 00 	vmovdqu 0xd0(%r10),%ymm10
   48265:	00 00
   48267:	c4 41 7e 6f 9a d8 00 	vmovdqu 0xd8(%r10),%ymm11
   4826e:	00 00
   48270:	c4 41 7e 6f a2 e0 00 	vmovdqu 0xe0(%r10),%ymm12
   48277:	00 00
   48279:	c4 41 7e 6f aa e8 00 	vmovdqu 0xe8(%r10),%ymm13
   48280:	00 00
   48282:	c4 41 7e 6f b2 f0 00 	vmovdqu 0xf0(%r10),%ymm14
   48289:	00 00
   4828b:	c4 41 7e 6f ba f8 00 	vmovdqu 0xf8(%r10),%ymm15
   48292:	00 00
   48294:	66 66 66 2e 0f 1f 84 	data16 data16 cs nopw 0x0(%rax,%rax,1)
   4829b:	00 00 00 00 00
   482a0:	4a 63 4c ce f8       	movslq -0x8(%rsi,%r9,8),%rcx
   482a5:	c4 e1 f9 6e c1       	vmovq  %rcx,%xmm0
   482aa:	c4 e2 7d 59 c0       	vpbroadcastq %xmm0,%ymm0
   482af:	c4 e2 7d 28 44 24 20 	vpmuldq 0x20(%rsp),%ymm0,%ymm0
   482b6:	4a 63 4c ce f0       	movslq -0x10(%rsi,%r9,8),%rcx
   482bb:	c4 e1 f9 6e c9       	vmovq  %rcx,%xmm1
   482c0:	c4 e2 7d 59 c9       	vpbroadcastq %xmm1,%ymm1
   482c5:	c4 e2 75 28 0c 24    	vpmuldq (%rsp),%ymm1,%ymm1
   482cb:	c5 f5 d4 c0          	vpaddq %ymm0,%ymm1,%ymm0
   482cf:	4a 63 4c ce e8       	movslq -0x18(%rsi,%r9,8),%rcx
   482d4:	c4 e1 f9 6e c9       	vmovq  %rcx,%xmm1
   482d9:	c4 e2 7d 59 c9       	vpbroadcastq %xmm1,%ymm1
   482de:	c4 e2 75 28 4c 24 e0 	vpmuldq -0x20(%rsp),%ymm1,%ymm1
   482e5:	4a 63 4c ce e0       	movslq -0x20(%rsi,%r9,8),%rcx
   482ea:	c4 e1 f9 6e d1       	vmovq  %rcx,%xmm2
   482ef:	c4 e2 7d 59 d2       	vpbroadcastq %xmm2,%ymm2
   482f4:	c4 e2 6d 28 54 24 c0 	vpmuldq -0x40(%rsp),%ymm2,%ymm2
   482fb:	c5 f5 d4 ca          	vpaddq %ymm2,%ymm1,%ymm1
   482ff:	c5 fd d4 c1          	vpaddq %ymm1,%ymm0,%ymm0
   48303:	4a 63 4c ce d8       	movslq -0x28(%rsi,%r9,8),%rcx
   48308:	c4 e1 f9 6e c9       	vmovq  %rcx,%xmm1
   4830d:	c4 e2 7d 59 c9       	vpbroadcastq %xmm1,%ymm1
   48312:	c4 e2 75 28 4c 24 a0 	vpmuldq -0x60(%rsp),%ymm1,%ymm1
   48319:	4a 63 4c ce d0       	movslq -0x30(%rsi,%r9,8),%rcx
   4831e:	c4 e1 f9 6e d1       	vmovq  %rcx,%xmm2
   48323:	c4 e2 7d 59 d2       	vpbroadcastq %xmm2,%ymm2
   48328:	c4 e2 6d 28 94 24 e0 	vpmuldq 0x1e0(%rsp),%ymm2,%ymm2
   4832f:	01 00 00
   48332:	c5 f5 d4 ca          	vpaddq %ymm2,%ymm1,%ymm1
   48336:	4a 63 4c ce c8       	movslq -0x38(%rsi,%r9,8),%rcx
   4833b:	c4 e1 f9 6e d1       	vmovq  %rcx,%xmm2
   48340:	c4 e2 7d 59 d2       	vpbroadcastq %xmm2,%ymm2
   48345:	c4 e2 6d 28 94 24 c0 	vpmuldq 0x1c0(%rsp),%ymm2,%ymm2
   4834c:	01 00 00
   4834f:	c5 f5 d4 ca          	vpaddq %ymm2,%ymm1,%ymm1
   48353:	c5 fd d4 c1          	vpaddq %ymm1,%ymm0,%ymm0
   48357:	4a 63 4c ce c0       	movslq -0x40(%rsi,%r9,8),%rcx
   4835c:	c4 e1 f9 6e c9       	vmovq  %rcx,%xmm1
   48361:	c4 e2 7d 59 c9       	vpbroadcastq %xmm1,%ymm1
   48366:	c4 e2 75 28 8c 24 a0 	vpmuldq 0x1a0(%rsp),%ymm1,%ymm1
   4836d:	01 00 00
   48370:	4a 63 4c ce b8       	movslq -0x48(%rsi,%r9,8),%rcx
   48375:	c4 e1 f9 6e d1       	vmovq  %rcx,%xmm2
   4837a:	c4 e2 7d 59 d2       	vpbroadcastq %xmm2,%ymm2
   4837f:	c4 e2 6d 28 94 24 80 	vpmuldq 0x180(%rsp),%ymm2,%ymm2
   48386:	01 00 00
   48389:	c5 f5 d4 ca          	vpaddq %ymm2,%ymm1,%ymm1
   4838d:	4a 63 4c ce b0       	movslq -0x50(%rsi,%r9,8),%rcx
   48392:	c4 e1 f9 6e d1       	vmovq  %rcx,%xmm2
   48397:	c4 e2 7d 59 d2       	vpbroadcastq %xmm2,%ymm2
   4839c:	c4 e2 6d 28 94 24 60 	vpmuldq 0x160(%rsp),%ymm2,%ymm2
   483a3:	01 00 00
   483a6:	c5 f5 d4 ca          	vpaddq %ymm2,%ymm1,%ymm1
   483aa:	4a 63 4c ce a8       	movslq -0x58(%rsi,%r9,8),%rcx
   483af:	c4 e1 f9 6e d1       	vmovq  %rcx,%xmm2
   483b4:	c4 e2 7d 59 d2       	vpbroadcastq %xmm2,%ymm2
   483b9:	c4 e2 6d 28 94 24 40 	vpmuldq 0x140(%rsp),%ymm2,%ymm2
   483c0:	01 00 00
   483c3:	c5 f5 d4 ca          	vpaddq %ymm2,%ymm1,%ymm1
   483c7:	c5 fd d4 c1          	vpaddq %ymm1,%ymm0,%ymm0
   483cb:	4a 63 4c ce a0       	movslq -0x60(%rsi,%r9,8),%rcx
   483d0:	c4 e1 f9 6e c9       	vmovq  %rcx,%xmm1
   483d5:	c4 e2 7d 59 c9       	vpbroadcastq %xmm1,%ymm1
   483da:	c4 e2 75 28 8c 24 20 	vpmuldq 0x120(%rsp),%ymm1,%ymm1
   483e1:	01 00 00
   483e4:	4a 63 4c ce 98       	movslq -0x68(%rsi,%r9,8),%rcx
   483e9:	c4 e1 f9 6e d1       	vmovq  %rcx,%xmm2
   483ee:	c4 e2 7d 59 d2       	vpbroadcastq %xmm2,%ymm2
   483f3:	c4 e2 6d 28 94 24 00 	vpmuldq 0x100(%rsp),%ymm2,%ymm2
   483fa:	01 00 00
   483fd:	c5 f5 d4 ca          	vpaddq %ymm2,%ymm1,%ymm1
   48401:	4a 63 4c ce 90       	movslq -0x70(%rsi,%r9,8),%rcx
   48406:	c4 e1 f9 6e d1       	vmovq  %rcx,%xmm2
   4840b:	c4 e2 7d 59 d2       	vpbroadcastq %xmm2,%ymm2
   48410:	c4 e2 6d 28 94 24 e0 	vpmuldq 0xe0(%rsp),%ymm2,%ymm2
   48417:	00 00 00
   4841a:	c5 f5 d4 ca          	vpaddq %ymm2,%ymm1,%ymm1
   4841e:	4a 63 4c ce 88       	movslq -0x78(%rsi,%r9,8),%rcx
   48423:	c4 e1 f9 6e d1       	vmovq  %rcx,%xmm2
   48428:	c4 e2 7d 59 d2       	vpbroadcastq %xmm2,%ymm2
   4842d:	c4 e2 6d 28 94 24 c0 	vpmuldq 0xc0(%rsp),%ymm2,%ymm2
   48434:	00 00 00
   48437:	c5 f5 d4 ca          	vpaddq %ymm2,%ymm1,%ymm1
   4843b:	4a 63 4c ce 80       	movslq -0x80(%rsi,%r9,8),%rcx
   48440:	c4 e1 f9 6e d1       	vmovq  %rcx,%xmm2
   48445:	c4 e2 7d 59 d2       	vpbroadcastq %xmm2,%ymm2
   4844a:	c4 e2 6d 28 94 24 a0 	vpmuldq 0xa0(%rsp),%ymm2,%ymm2
   48451:	00 00 00
   48454:	c5 f5 d4 ca          	vpaddq %ymm2,%ymm1,%ymm1
   48458:	c5 fd d4 c1          	vpaddq %ymm1,%ymm0,%ymm0
   4845c:	4a 63 8c ce 78 ff ff 	movslq -0x88(%rsi,%r9,8),%rcx
   48463:	ff
   48464:	c4 e1 f9 6e c9       	vmovq  %rcx,%xmm1
   48469:	c4 e2 7d 59 c9       	vpbroadcastq %xmm1,%ymm1
   4846e:	c4 e2 75 28 8c 24 80 	vpmuldq 0x80(%rsp),%ymm1,%ymm1
   48475:	00 00 00
   48478:	4a 63 8c ce 70 ff ff 	movslq -0x90(%rsi,%r9,8),%rcx
   4847f:	ff
   48480:	c4 e1 f9 6e d1       	vmovq  %rcx,%xmm2
   48485:	c4 e2 7d 59 d2       	vpbroadcastq %xmm2,%ymm2
   4848a:	c4 e2 6d 28 54 24 60 	vpmuldq 0x60(%rsp),%ymm2,%ymm2
   48491:	c5 f5 d4 ca          	vpaddq %ymm2,%ymm1,%ymm1
   48495:	4a 63 8c ce 68 ff ff 	movslq -0x98(%rsi,%r9,8),%rcx
   4849c:	ff
   4849d:	c4 e1 f9 6e d1       	vmovq  %rcx,%xmm2
   484a2:	c4 e2 7d 59 d2       	vpbroadcastq %xmm2,%ymm2
   484a7:	c4 e2 6d 28 54 24 40 	vpmuldq 0x40(%rsp),%ymm2,%ymm2
   484ae:	c5 f5 d4 ca          	vpaddq %ymm2,%ymm1,%ymm1
   484b2:	4a 63 8c ce 60 ff ff 	movslq -0xa0(%rsi,%r9,8),%rcx
   484b9:	ff
   484ba:	c4 e1 f9 6e d1       	vmovq  %rcx,%xmm2
   484bf:	c4 e2 7d 59 d2       	vpbroadcastq %xmm2,%ymm2
   484c4:	c4 e2 6d 28 d3       	vpmuldq %ymm3,%ymm2,%ymm2
   484c9:	c5 f5 d4 ca          	vpaddq %ymm2,%ymm1,%ymm1
   484cd:	4a 63 8c ce 58 ff ff 	movslq -0xa8(%rsi,%r9,8),%rcx
   484d4:	ff
   484d5:	c4 e1 f9 6e d1       	vmovq  %rcx,%xmm2
   484da:	c4 e2 7d 59 d2       	vpbroadcastq %xmm2,%ymm2
   484df:	c4 e2 6d 28 d4       	vpmuldq %ymm4,%ymm2,%ymm2
   484e4:	c5 f5 d4 ca          	vpaddq %ymm2,%ymm1,%ymm1
   484e8:	4a 63 8c ce 50 ff ff 	movslq -0xb0(%rsi,%r9,8),%rcx
   484ef:	ff
   484f0:	c4 e1 f9 6e d1       	vmovq  %rcx,%xmm2
   484f5:	c4 e2 7d 59 d2       	vpbroadcastq %xmm2,%ymm2
   484fa:	c4 e2 6d 28 d5       	vpmuldq %ymm5,%ymm2,%ymm2
   484ff:	c5 f5 d4 ca          	vpaddq %ymm2,%ymm1,%ymm1
   48503:	c5 fd d4 c1          	vpaddq %ymm1,%ymm0,%ymm0
   48507:	4a 63 8c ce 48 ff ff 	movslq -0xb8(%rsi,%r9,8),%rcx
   4850e:	ff
   4850f:	c4 e1 f9 6e c9       	vmovq  %rcx,%xmm1
   48514:	c4 e2 7d 59 c9       	vpbroadcastq %xmm1,%ymm1
   48519:	c4 e2 75 28 ce       	vpmuldq %ymm6,%ymm1,%ymm1
   4851e:	4a 63 8c ce 40 ff ff 	movslq -0xc0(%rsi,%r9,8),%rcx
   48525:	ff
   48526:	c4 e1 f9 6e d1       	vmovq  %rcx,%xmm2
   4852b:	c4 e2 7d 59 d2       	vpbroadcastq %xmm2,%ymm2
   48530:	c4 e2 6d 28 d7       	vpmuldq %ymm7,%ymm2,%ymm2
   48535:	c5 f5 d4 ca          	vpaddq %ymm2,%ymm1,%ymm1
   48539:	4a 63 8c ce 38 ff ff 	movslq -0xc8(%rsi,%r9,8),%rcx
   48540:	ff
   48541:	c4 e1 f9 6e d1       	vmovq  %rcx,%xmm2
   48546:	c4 e2 7d 59 d2       	vpbroadcastq %xmm2,%ymm2
   4854b:	c4 c2 6d 28 d0       	vpmuldq %ymm8,%ymm2,%ymm2
   48550:	c5 f5 d4 ca          	vpaddq %ymm2,%ymm1,%ymm1
   48554:	4a 63 8c ce 30 ff ff 	movslq -0xd0(%rsi,%r9,8),%rcx
   4855b:	ff
   4855c:	c4 e1 f9 6e d1       	vmovq  %rcx,%xmm2
   48561:	c4 e2 7d 59 d2       	vpbroadcastq %xmm2,%ymm2
   48566:	c4 c2 6d 28 d1       	vpmuldq %ymm9,%ymm2,%ymm2
   4856b:	c5 f5 d4 ca          	vpaddq %ymm2,%ymm1,%ymm1
   4856f:	4a 63 8c ce 28 ff ff 	movslq -0xd8(%rsi,%r9,8),%rcx
   48576:	ff
   48577:	c4 e1 f9 6e d1       	vmovq  %rcx,%xmm2
   4857c:	c4 e2 7d 59 d2       	vpbroadcastq %xmm2,%ymm2
   48581:	c4 c2 6d 28 d2       	vpmuldq %ymm10,%ymm2,%ymm2
   48586:	c5 f5 d4 ca          	vpaddq %ymm2,%ymm1,%ymm1
   4858a:	4a 63 8c ce 20 ff ff 	movslq -0xe0(%rsi,%r9,8),%rcx
   48591:	ff
   48592:	c4 e1 f9 6e d1       	vmovq  %rcx,%xmm2
   48597:	c4 e2 7d 59 d2       	vpbroadcastq %xmm2,%ymm2
   4859c:	c4 c2 6d 28 d3       	vpmuldq %ymm11,%ymm2,%ymm2
   485a1:	c5 f5 d4 ca          	vpaddq %ymm2,%ymm1,%ymm1
   485a5:	4a 63 8c ce 18 ff ff 	movslq -0xe8(%rsi,%r9,8),%rcx
   485ac:	ff
   485ad:	c4 e1 f9 6e d1       	vmovq  %rcx,%xmm2
   485b2:	c4 e2 7d 59 d2       	vpbroadcastq %xmm2,%ymm2
   485b7:	c4 c2 6d 28 d4       	vpmuldq %ymm12,%ymm2,%ymm2
   485bc:	c5 f5 d4 ca          	vpaddq %ymm2,%ymm1,%ymm1
   485c0:	c5 fd d4 c1          	vpaddq %ymm1,%ymm0,%ymm0
   485c4:	4a 63 8c ce 10 ff ff 	movslq -0xf0(%rsi,%r9,8),%rcx
   485cb:	ff
   485cc:	c4 e1 f9 6e c9       	vmovq  %rcx,%xmm1
   485d1:	c4 e2 7d 59 c9       	vpbroadcastq %xmm1,%ymm1
   485d6:	c4 c2 75 28 cd       	vpmuldq %ymm13,%ymm1,%ymm1
   485db:	4a 63 8c ce 08 ff ff 	movslq -0xf8(%rsi,%r9,8),%rcx
   485e2:	ff
   485e3:	c4 e1 f9 6e d1       	vmovq  %rcx,%xmm2
   485e8:	c4 e2 7d 59 d2       	vpbroadcastq %xmm2,%ymm2
   485ed:	c4 c2 6d 28 d6       	vpmuldq %ymm14,%ymm2,%ymm2
   485f2:	c5 f5 d4 ca          	vpaddq %ymm2,%ymm1,%ymm1
   485f6:	4a 63 8c ce 00 ff ff 	movslq -0x100(%rsi,%r9,8),%rcx
   485fd:	ff
   485fe:	c4 e1 f9 6e d1       	vmovq  %rcx,%xmm2
   48603:	c4 e2 7d 59 d2       	vpbroadcastq %xmm2,%ymm2
   48608:	c4 c2 6d 28 d7       	vpmuldq %ymm15,%ymm2,%ymm2
   4860d:	c5 f5 d4 ca          	vpaddq %ymm2,%ymm1,%ymm1
   48611:	c5 fd d4 c1          	vpaddq %ymm1,%ymm0,%ymm0
   48615:	c4 c1 f9 7e c7       	vmovq  %xmm0,%r15
   4861a:	89 c1                	mov    %eax,%ecx
   4861c:	49 d3 ff             	sar    %cl,%r15
   4861f:	c4 e3 f9 16 c1 01    	vpextrq $0x1,%xmm0,%rcx
   48625:	c4 e3 7d 39 c0 01    	vextracti128 $0x1,%ymm0,%xmm0
   4862b:	c4 c3 f9 16 c0 01    	vpextrq $0x1,%xmm0,%r8
   48631:	c4 e1 f9 7e c7       	vmovq  %xmm0,%rdi
   48636:	4e 03 3c ce          	add    (%rsi,%r9,8),%r15
   4863a:	4b 8d 2c 1f          	lea    (%r15,%r11,1),%rbp
   4863e:	4e 89 3c ce          	mov    %r15,(%rsi,%r9,8)
   48642:	4d 89 fd             	mov    %r15,%r13
   48645:	4c 0f af eb          	imul   %rbx,%r13
   48649:	49 01 cd             	add    %rcx,%r13
   4864c:	89 c1                	mov    %eax,%ecx
   4864e:	49 d3 fd             	sar    %cl,%r13
   48651:	4e 03 6c ce 08       	add    0x8(%rsi,%r9,8),%r13
   48656:	4c 89 fa             	mov    %r15,%rdx
   48659:	49 0f af d6          	imul   %r14,%rdx
   4865d:	48 01 fa             	add    %rdi,%rdx
   48660:	4c 89 e9             	mov    %r13,%rcx
   48663:	48 0f af cb          	imul   %rbx,%rcx
   48667:	48 01 ca             	add    %rcx,%rdx
   4866a:	89 c1                	mov    %eax,%ecx
   4866c:	48 d3 fa             	sar    %cl,%rdx
   4866f:	4b 8d 3c 2b          	lea    (%r11,%r13,1),%rdi
   48673:	4e 89 6c ce 08       	mov    %r13,0x8(%rsi,%r9,8)
   48678:	4a 03 54 ce 10       	add    0x10(%rsi,%r9,8),%rdx
   4867d:	4c 0f af 7c 24 98    	imul   -0x68(%rsp),%r15
   48683:	4d 01 c7             	add    %r8,%r15
   48686:	4e 8d 04 1a          	lea    (%rdx,%r11,1),%r8
   4868a:	4a 89 54 ce 10       	mov    %rdx,0x10(%rsi,%r9,8)
   4868f:	48 0f af d3          	imul   %rbx,%rdx
   48693:	4d 0f af ee          	imul   %r14,%r13
   48697:	4d 01 ef             	add    %r13,%r15
   4869a:	49 01 d7             	add    %rdx,%r15
   4869d:	49 d3 ff             	sar    %cl,%r15
   486a0:	48 09 ef             	or     %rbp,%rdi
   486a3:	49 09 f8             	or     %rdi,%r8
   486a6:	4e 03 7c ce 18       	add    0x18(%rsi,%r9,8),%r15
   486ab:	4b 8d 14 1f          	lea    (%r15,%r11,1),%rdx
   486af:	4c 09 c2             	or     %r8,%rdx
   486b2:	4e 89 7c ce 18       	mov    %r15,0x18(%rsi,%r9,8)
   486b7:	48 8b 4c 24 88       	mov    -0x78(%rsp),%rcx
   486bc:	48 d3 ea             	shr    %cl,%rdx
   486bf:	48 85 d2             	test   %rdx,%rdx
   486c2:	0f 85 83 02 00 00    	jne    4894b <_RINvNtNtCs6l4HYnHGbZv_25deadlib_audio_decode_flac9predictor3x866kernelKj20_EB6_+0x88b>
   486c8:	49 83 c1 04          	add    $0x4,%r9
   486cc:	49 83 c4 fc          	add    $0xfffffffffffffffc,%r12
   486d0:	49 83 fc 04          	cmp    $0x4,%r12
   486d4:	0f 83 c6 fb ff ff    	jae    482a0 <_RINvNtNtCs6l4HYnHGbZv_25deadlib_audio_decode_flac9predictor3x866kernelKj20_EB6_+0x1e0>
   486da:	48 8b 54 24 90       	mov    -0x70(%rsp),%rdx
   486df:	49 39 d1             	cmp    %rdx,%r9
   486e2:	0f 83 55 02 00 00    	jae    4893d <_RINvNtNtCs6l4HYnHGbZv_25deadlib_audio_decode_flac9predictor3x866kernelKj20_EB6_+0x87d>
   486e8:	c4 c3 fd 00 02 1b    	vpermq $0x1b,(%r10),%ymm0
   486ee:	c4 c3 fd 00 4a 20 1b 	vpermq $0x1b,0x20(%r10),%ymm1
   486f5:	c4 c3 fd 00 52 40 1b 	vpermq $0x1b,0x40(%r10),%ymm2
   486fc:	c4 c3 fd 00 5a 60 1b 	vpermq $0x1b,0x60(%r10),%ymm3
   48703:	c4 c3 fd 00 a2 80 00 	vpermq $0x1b,0x80(%r10),%ymm4
   4870a:	00 00 1b
   4870d:	c4 c3 fd 00 aa a0 00 	vpermq $0x1b,0xa0(%r10),%ymm5
   48714:	00 00 1b
   48717:	c4 c3 fd 00 b2 c0 00 	vpermq $0x1b,0xc0(%r10),%ymm6
   4871e:	00 00 1b
   48721:	c4 c3 fd 00 ba e0 00 	vpermq $0x1b,0xe0(%r10),%ymm7
   48728:	00 00 1b
   4872b:	c5 bd 73 d5 20       	vpsrlq $0x20,%ymm5,%ymm8
   48730:	c5 7e 7f 44 24 20    	vmovdqu %ymm8,0x20(%rsp)
   48736:	c5 bd 73 d1 20       	vpsrlq $0x20,%ymm1,%ymm8
   4873b:	c5 7e 7f 04 24       	vmovdqu %ymm8,(%rsp)
   48740:	c5 bd 73 d7 20       	vpsrlq $0x20,%ymm7,%ymm8
   48745:	c5 7e 7f 44 24 e0    	vmovdqu %ymm8,-0x20(%rsp)
   4874b:	c5 bd 73 d3 20       	vpsrlq $0x20,%ymm3,%ymm8
   48750:	c5 7e 7f 44 24 c0    	vmovdqu %ymm8,-0x40(%rsp)
   48756:	c5 bd 73 d4 20       	vpsrlq $0x20,%ymm4,%ymm8
   4875b:	c5 7e 7f 44 24 a0    	vmovdqu %ymm8,-0x60(%rsp)
   48761:	c5 95 73 d0 20       	vpsrlq $0x20,%ymm0,%ymm13
   48766:	c5 8d 73 d6 20       	vpsrlq $0x20,%ymm6,%ymm14
   4876b:	0f 1f 44 00 00       	nopl   0x0(%rax,%rax,1)
   48770:	c4 21 7e 6f 7c ce a0 	vmovdqu -0x60(%rsi,%r9,8),%ymm15
   48777:	c4 21 7e 6f 44 ce 80 	vmovdqu -0x80(%rsi,%r9,8),%ymm8
   4877e:	c4 21 7e 6f 4c ce c0 	vmovdqu -0x40(%rsi,%r9,8),%ymm9
   48785:	c4 21 7e 6f 94 ce 40 	vmovdqu -0xc0(%rsi,%r9,8),%ymm10
   4878c:	ff ff ff
   4878f:	c4 c1 25 73 d2 20    	vpsrlq $0x20,%ymm10,%ymm11
   48795:	c5 25 f4 dd          	vpmuludq %ymm5,%ymm11,%ymm11
   48799:	c5 2d f4 64 24 20    	vpmuludq 0x20(%rsp),%ymm10,%ymm12
   4879f:	c4 41 1d d4 db       	vpaddq %ymm11,%ymm12,%ymm11
   487a4:	c4 c1 25 73 f3 20    	vpsllq $0x20,%ymm11,%ymm11
   487aa:	c5 2d f4 d5          	vpmuludq %ymm5,%ymm10,%ymm10
   487ae:	c4 41 2d d4 d3       	vpaddq %ymm11,%ymm10,%ymm10
   487b3:	c4 c1 25 73 d1 20    	vpsrlq $0x20,%ymm9,%ymm11
   487b9:	c5 25 f4 d9          	vpmuludq %ymm1,%ymm11,%ymm11
   487bd:	c5 35 f4 24 24       	vpmuludq (%rsp),%ymm9,%ymm12
   487c2:	c4 41 1d d4 db       	vpaddq %ymm11,%ymm12,%ymm11
   487c7:	c4 21 7e 6f a4 ce 00 	vmovdqu -0x100(%rsi,%r9,8),%ymm12
   487ce:	ff ff ff
   487d1:	c4 c1 25 73 f3 20    	vpsllq $0x20,%ymm11,%ymm11
   487d7:	c5 35 f4 c9          	vpmuludq %ymm1,%ymm9,%ymm9
   487db:	c4 41 35 d4 cb       	vpaddq %ymm11,%ymm9,%ymm9
   487e0:	c4 41 2d d4 c9       	vpaddq %ymm9,%ymm10,%ymm9
   487e5:	c5 1d f4 54 24 e0    	vpmuludq -0x20(%rsp),%ymm12,%ymm10
   487eb:	c4 c1 25 73 d4 20    	vpsrlq $0x20,%ymm12,%ymm11
   487f1:	c5 25 f4 df          	vpmuludq %ymm7,%ymm11,%ymm11
   487f5:	c4 41 2d d4 d3       	vpaddq %ymm11,%ymm10,%ymm10
   487fa:	c4 c1 2d 73 f2 20    	vpsllq $0x20,%ymm10,%ymm10
   48800:	c5 1d f4 df          	vpmuludq %ymm7,%ymm12,%ymm11
   48804:	c4 41 25 d4 d2       	vpaddq %ymm10,%ymm11,%ymm10
   48809:	c4 c1 25 73 d0 20    	vpsrlq $0x20,%ymm8,%ymm11
   4880f:	c5 25 f4 db          	vpmuludq %ymm3,%ymm11,%ymm11
   48813:	c5 3d f4 64 24 c0    	vpmuludq -0x40(%rsp),%ymm8,%ymm12
   48819:	c4 41 1d d4 db       	vpaddq %ymm11,%ymm12,%ymm11
   4881e:	c4 21 7e 6f 64 ce e0 	vmovdqu -0x20(%rsi,%r9,8),%ymm12
   48825:	c4 c1 25 73 f3 20    	vpsllq $0x20,%ymm11,%ymm11
   4882b:	c5 3d f4 c3          	vpmuludq %ymm3,%ymm8,%ymm8
   4882f:	c4 41 3d d4 c3       	vpaddq %ymm11,%ymm8,%ymm8
   48834:	c4 21 7e 6f 9c ce 60 	vmovdqu -0xa0(%rsi,%r9,8),%ymm11
   4883b:	ff ff ff
   4883e:	c4 41 2d d4 c0       	vpaddq %ymm8,%ymm10,%ymm8
   48843:	c4 41 3d d4 c1       	vpaddq %ymm9,%ymm8,%ymm8
   48848:	c4 c1 35 73 d3 20    	vpsrlq $0x20,%ymm11,%ymm9
   4884e:	c5 35 f4 cc          	vpmuludq %ymm4,%ymm9,%ymm9
   48852:	c5 25 f4 54 24 a0    	vpmuludq -0x60(%rsp),%ymm11,%ymm10
   48858:	c4 41 2d d4 c9       	vpaddq %ymm9,%ymm10,%ymm9
   4885d:	c4 c1 35 73 f1 20    	vpsllq $0x20,%ymm9,%ymm9
   48863:	c5 25 f4 d4          	vpmuludq %ymm4,%ymm11,%ymm10
   48867:	c4 41 2d d4 c9       	vpaddq %ymm9,%ymm10,%ymm9
   4886c:	c4 c1 2d 73 d4 20    	vpsrlq $0x20,%ymm12,%ymm10
   48872:	c5 2d f4 d0          	vpmuludq %ymm0,%ymm10,%ymm10
   48876:	c4 41 1d f4 dd       	vpmuludq %ymm13,%ymm12,%ymm11
   4887b:	c4 41 25 d4 d2       	vpaddq %ymm10,%ymm11,%ymm10
   48880:	c4 21 7e 6f 9c ce 20 	vmovdqu -0xe0(%rsi,%r9,8),%ymm11
   48887:	ff ff ff
   4888a:	c4 c1 2d 73 f2 20    	vpsllq $0x20,%ymm10,%ymm10
   48890:	c5 1d f4 e0          	vpmuludq %ymm0,%ymm12,%ymm12
   48894:	c4 41 1d d4 d2       	vpaddq %ymm10,%ymm12,%ymm10
   48899:	c4 41 35 d4 ca       	vpaddq %ymm10,%ymm9,%ymm9
   4889e:	c4 c1 2d 73 d3 20    	vpsrlq $0x20,%ymm11,%ymm10
   488a4:	c5 2d f4 d6          	vpmuludq %ymm6,%ymm10,%ymm10
   488a8:	c4 41 25 f4 e6       	vpmuludq %ymm14,%ymm11,%ymm12
   488ad:	c4 41 1d d4 d2       	vpaddq %ymm10,%ymm12,%ymm10
   488b2:	c4 c1 2d 73 f2 20    	vpsllq $0x20,%ymm10,%ymm10
   488b8:	c5 25 f4 de          	vpmuludq %ymm6,%ymm11,%ymm11
   488bc:	c4 41 25 d4 d2       	vpaddq %ymm10,%ymm11,%ymm10
   488c1:	c4 c1 25 73 d7 20    	vpsrlq $0x20,%ymm15,%ymm11
   488c7:	c5 25 f4 da          	vpmuludq %ymm2,%ymm11,%ymm11
   488cb:	c5 9d 73 d2 20       	vpsrlq $0x20,%ymm2,%ymm12
   488d0:	c4 41 05 f4 e4       	vpmuludq %ymm12,%ymm15,%ymm12
   488d5:	c4 41 1d d4 db       	vpaddq %ymm11,%ymm12,%ymm11
   488da:	c4 c1 25 73 f3 20    	vpsllq $0x20,%ymm11,%ymm11
   488e0:	c5 05 f4 e2          	vpmuludq %ymm2,%ymm15,%ymm12
   488e4:	c4 41 1d d4 db       	vpaddq %ymm11,%ymm12,%ymm11
   488e9:	c4 41 2d d4 d3       	vpaddq %ymm11,%ymm10,%ymm10
   488ee:	c4 41 2d d4 c9       	vpaddq %ymm9,%ymm10,%ymm9
   488f3:	c4 41 3d d4 c1       	vpaddq %ymm9,%ymm8,%ymm8
   488f8:	c4 43 7d 39 c1 01    	vextracti128 $0x1,%ymm8,%xmm9
   488fe:	c4 41 39 d4 c1       	vpaddq %xmm9,%xmm8,%xmm8
   48903:	c4 41 79 70 c8 ee    	vpshufd $0xee,%xmm8,%xmm9
   48909:	c4 41 39 d4 c1       	vpaddq %xmm9,%xmm8,%xmm8
   4890e:	c4 41 f9 7e c2       	vmovq  %xmm8,%r10
   48913:	89 c1                	mov    %eax,%ecx
   48915:	49 d3 fa             	sar    %cl,%r10
   48918:	4e 03 14 ce          	add    (%rsi,%r9,8),%r10
   4891c:	4b 8d 1c 1a          	lea    (%r10,%r11,1),%rbx
   48920:	48 8b 4c 24 88       	mov    -0x78(%rsp),%rcx
   48925:	48 d3 eb             	shr    %cl,%rbx
   48928:	48 85 db             	test   %rbx,%rbx
   4892b:	75 1e                	jne    4894b <_RINvNtNtCs6l4HYnHGbZv_25deadlib_audio_decode_flac9predictor3x866kernelKj20_EB6_+0x88b>
   4892d:	4e 89 14 ce          	mov    %r10,(%rsi,%r9,8)
   48931:	49 ff c1             	inc    %r9
   48934:	4c 39 ca             	cmp    %r9,%rdx
   48937:	0f 85 33 fe ff ff    	jne    48770 <_RINvNtNtCs6l4HYnHGbZv_25deadlib_audio_decode_flac9predictor3x866kernelKj20_EB6_+0x6b0>
   4893d:	48 8b 44 24 80       	mov    -0x80(%rsp),%rax
   48942:	48 c7 00 ff ff ff ff 	movq   $0xffffffffffffffff,(%rax)
   48949:	eb 1f                	jmp    4896a <_RINvNtNtCs6l4HYnHGbZv_25deadlib_audio_decode_flac9predictor3x866kernelKj20_EB6_+0x8aa>
   4894b:	48 8b 4c 24 80       	mov    -0x80(%rsp),%rcx
   48950:	48 c7 01 01 00 00 00 	movq   $0x1,(%rcx)
   48957:	48 8d 05 6c 5a fc ff 	lea    -0x3a594(%rip),%rax        # e3ca <_RNvNtNtNtCskADBYZwJmSU_4core7unicode12unicode_data2cf17SHORT_OFFSET_RUNS+0xaba>
   4895e:	48 89 41 08          	mov    %rax,0x8(%rcx)
   48962:	48 c7 41 10 21 00 00 	movq   $0x21,0x10(%rcx)
   48969:	00
   4896a:	48 81 c4 08 02 00 00 	add    $0x208,%rsp
   48971:	5b                   	pop    %rbx
   48972:	41 5c                	pop    %r12
   48974:	41 5d                	pop    %r13
   48976:	41 5e                	pop    %r14
   48978:	41 5f                	pop    %r15
   4897a:	5d                   	pop    %rbp
   4897b:	c5 f8 77             	vzeroupper
   4897e:	c3                   	ret

Disassembly of section .init:

Disassembly of section .fini:

Disassembly of section .plt:
