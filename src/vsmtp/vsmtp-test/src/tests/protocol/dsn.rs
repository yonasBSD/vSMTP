/*
 * vSMTP mail transfer agent
 * Copyright (C) 2022 viridIT SAS
 *
 * This program is free software: you can redistribute it and/or modify it under
 * the terms of the GNU General Public License as published by the Free Software
 * Foundation, either version 3 of the License, or any later version.
 *
 * This program is distributed in the hope that it will be useful, but WITHOUT
 * ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS
 * FOR A PARTICULAR PURPOSE.  See the GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License along with
 * this program. If not, see https://www.gnu.org/licenses/.
 *
*/

use crate::run_test;

run_test! {
    fn submission,
    input = [
        "EHLO Example.ORG\r\n",
        "MAIL FROM:<Alice@Example.ORG> RET=HDRS ENVID=QQ314159\r\n",
        "RCPT TO:<Bob@Example.COM> NOTIFY=SUCCESS ORCPT=rfc822;Bob@Example.COM\r\n",
        "RCPT TO:<Carol@Ivory.EDU> NOTIFY=FAILURE ORCPT=rfc822;Carol@Ivory.EDU\r\n",
        "RCPT TO:<Dana@Ivory.EDU> NOTIFY=SUCCESS,FAILURE ORCPT=rfc822;Dana@Ivory.EDU\r\n",
        "RCPT TO:<Eric@Bombs.AF.MIL> NOTIFY=FAILURE ORCPT=rfc822;Eric@Bombs.AF.MIL\r\n",
        "RCPT TO:<Fred@Bombs.AF.MIL> NOTIFY=NEVER\r\n",
        "RCPT TO:<George@Tax-ME.GOV> NOTIFY=FAILURE ORCPT=rfc822;George@Tax-ME.GOV\r\n",
        "DATA\r\n",
        ".\r\n",
        "QUIT\r\n"
    ],
    expected = [
        "220 testserver.com Service ready\r\n",
        "250-testserver.com\r\n",
        "250-8BITMIME\r\n",
        "250-SMTPUTF8\r\n",
        "250-STARTTLS\r\n",
        "250-PIPELINING\r\n",
        "250-DSN\r\n",
        "250 SIZE 20000000\r\n",
        "250 Ok\r\n",
        "250 Ok\r\n",
        "250 Ok\r\n",
        "250 Ok\r\n",
        "250 Ok\r\n",
        "250 Ok\r\n",
        "250 Ok\r\n",
        "354 Start mail input; end with <CRLF>.<CRLF>\r\n",
        "250 Ok\r\n",
        "221 Service closing transmission channel\r\n",
    ],
}

run_test! {
    fn relay_one,
    input = [
        "EHLO Example.ORG\r\n",
        "MAIL FROM:<Alice@Example.ORG> RET=HDRS ENVID=QQ314159\r\n",
        "RCPT TO:<Bob@Example.COM> NOTIFY=SUCCESS ORCPT=rfc822;Bob@Example.COM\r\n",
        "DATA\r\n",
        ".\r\n",
        "QUIT\r\n",
    ],
    expected = [
        "220 testserver.com Service ready\r\n",
        "250-testserver.com\r\n",
        "250-8BITMIME\r\n",
        "250-SMTPUTF8\r\n",
        "250-STARTTLS\r\n",
        "250-PIPELINING\r\n",
        "250-DSN\r\n",
        "250 SIZE 20000000\r\n",
        "250 Ok\r\n",
        "250 Ok\r\n",
        "354 Start mail input; end with <CRLF>.<CRLF>\r\n",
        "250 Ok\r\n",
        "221 Service closing transmission channel\r\n",
    ],
}

run_test! {
    fn relay_two,
    input = [
        "EHLO Example.ORG\r\n",
        "MAIL FROM:<Alice@Example.ORG> RET=HDRS ENVID=QQ314159\r\n",
        "RCPT TO:<Carol@Ivory.EDU> NOTIFY=FAILURE ORCPT=rfc822;Carol@Ivory.EDU\r\n",
        "RCPT TO:<Dana@Ivory.EDU> NOTIFY=SUCCESS,FAILURE ORCPT=rfc822;Dana@Ivory.EDU\r\n",
        "DATA\r\n",
        ".\r\n",
        "QUIT\r\n",
    ],
    expected = [
        "220 testserver.com Service ready\r\n",
        "250-testserver.com\r\n",
        "250-8BITMIME\r\n",
        "250-SMTPUTF8\r\n",
        "250-STARTTLS\r\n",
        "250-PIPELINING\r\n",
        "250-DSN\r\n",
        "250 SIZE 20000000\r\n",
        "250 Ok\r\n",
        "250 Ok\r\n",
        "250 Ok\r\n",
        "354 Start mail input; end with <CRLF>.<CRLF>\r\n",
        "250 Ok\r\n",
        "221 Service closing transmission channel\r\n",
    ],
}

/*
run_test! {
    fn relay_three,
    input = [
        "EHLO Example.ORG\r\n",
        "RSET\r\n",
        "HELO Example.ORG\r\n",
        "MAIL FROM:<Alice@Example.ORG>\r\n",
        "RCPT TO:<Eric@Bombs.AF.MIL>\r\n",
        "DATA\r\n",
        ".\r\n",
        "MAIL FROM:<>\r\n",
        "RCPT TO:<Fred@Bombs.AF.MIL>\r\n",
        "DATA\r\n",
        ".\r\n",
        "QUIT\r\n",
    ],
    expected = [
        "220-Bombs.AF.MIL reporting for duty.\r\n",
        "220 Electronic mail is to be used for official business only.\r\n",
        "502 command not implemented\r\n",
        "250 reset\r\n",
        "250 Bombs.AF.MIL\r\n",
        "250 ok\r\n",
        "250 ok\r\n",
        "354 send message\r\n",
        "250 message accepted\r\n",
        "250 ok\r\n",
        "250 ok\r\n",
        "354 send message\r\n",
        "250 message accepted\r\n",
        "221 Bombs.AF.MIL closing connection\r\n",
    ],
}
*/
