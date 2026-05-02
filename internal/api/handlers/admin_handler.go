package handlers

import (
	"bytes"
	"crypto/tls"
	"encoding/json"
	"fmt"
	"io"
	"io/fs"
	"net/http"
	"net/smtp"
	"net/url"
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"sync"
	"time"

	"github.com/abdullaabdullazade/freedrive/internal/api/middleware"
	"github.com/abdullaabdullazade/freedrive/internal/domain"
	"github.com/abdullaabdullazade/freedrive/internal/repository"
	"github.com/abdullaabdullazade/freedrive/internal/service"
	"github.com/go-chi/chi/v5"
)

var (
	adminSettings   = map[string]interface{}{}
	adminSettingsMu sync.RWMutex
	settingsFile    = "data/settings.json"
)

func init() {
	loadSettings()
}

func loadSettings() {
	adminSettingsMu.Lock()
	defer adminSettingsMu.Unlock()

	if err := os.MkdirAll(filepath.Dir(settingsFile), 0755); err != nil {
		return
	}

	file, err := os.Open(settingsFile)
	if err != nil {
		return
	}
	defer file.Close()

	bytes, err := io.ReadAll(file)
	if err != nil {
		return
	}

	_ = json.Unmarshal(bytes, &adminSettings)
}

func saveSettings() {
	adminSettingsMu.RLock()
	defer adminSettingsMu.RUnlock()

	bytes, err := json.MarshalIndent(adminSettings, "", "  ")
	if err != nil {
		return
	}

	_ = os.WriteFile(settingsFile, bytes, 0644)
}

// AdminHandler handles admin endpoints.
type AdminHandler struct {
	userRepo     repository.UserRepository
	fileRepo     repository.FileRepository
	activityRepo repository.ActivityRepository
	authService  *service.AuthService
}

// NewAdminHandler creates a new admin handler.
func NewAdminHandler(userRepo repository.UserRepository, fileRepo repository.FileRepository, activityRepo repository.ActivityRepository, authService *service.AuthService) *AdminHandler {
	return &AdminHandler{
		userRepo:     userRepo,
		fileRepo:     fileRepo,
		activityRepo: activityRepo,
		authService:  authService,
	}
}

// ListUsers handles GET /api/v1/admin/users
func (h *AdminHandler) ListUsers(w http.ResponseWriter, r *http.Request) {
	users, err := h.userRepo.List(r.Context())
	if err != nil {
		writeError(w, "failed to list users", http.StatusInternalServerError)
		return
	}
	writeJSON(w, http.StatusOK, map[string]interface{}{"users": users})
}

// CreateUser handles POST /api/v1/admin/users
func (h *AdminHandler) CreateUser(w http.ResponseWriter, r *http.Request) {
	var req struct {
		Email    string `json:"email"`
		Username string `json:"username"`
		Password string `json:"password"`
		Role     string `json:"role"`
	}
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		writeError(w, "invalid request body", http.StatusBadRequest)
		return
	}

	user, err := h.authService.Register(r.Context(), req.Email, req.Username, req.Password, "")
	if err != nil {
		writeError(w, err.Error(), http.StatusBadRequest)
		return
	}

	// Update role if specified
	if req.Role != "" && req.Role != "user" {
		user.Role = domain.Role(req.Role)
		_ = h.userRepo.Update(r.Context(), user)
	}

	writeJSON(w, http.StatusCreated, user)
}

// UpdateUser handles PATCH /api/v1/admin/users/{id}
func (h *AdminHandler) UpdateUser(w http.ResponseWriter, r *http.Request) {
	userID := chi.URLParam(r, "id")

	user, err := h.userRepo.GetByID(r.Context(), userID)
	if err != nil || user == nil {
		writeError(w, "user not found", http.StatusNotFound)
		return
	}

	var req struct {
		Role       *string `json:"role"`
		QuotaBytes *int64  `json:"quota_bytes"`
		Username   *string `json:"username"`
	}
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		writeError(w, "invalid request body", http.StatusBadRequest)
		return
	}

	if req.Role != nil {
		user.Role = domain.Role(*req.Role)
	}
	if req.QuotaBytes != nil {
		user.QuotaBytes = *req.QuotaBytes
	}
	if req.Username != nil {
		user.Username = *req.Username
	}

	if err := h.userRepo.Update(r.Context(), user); err != nil {
		writeError(w, "failed to update user", http.StatusInternalServerError)
		return
	}

	writeJSON(w, http.StatusOK, user)
}

// DeleteUser handles DELETE /api/v1/admin/users/{id}
func (h *AdminHandler) DeleteUser(w http.ResponseWriter, r *http.Request) {
	userID := chi.URLParam(r, "id")
	currentUserID := middleware.GetUserID(r.Context())

	if userID == currentUserID {
		writeError(w, "cannot delete yourself", http.StatusBadRequest)
		return
	}

	if err := h.userRepo.Delete(r.Context(), userID); err != nil {
		writeError(w, "failed to delete user", http.StatusInternalServerError)
		return
	}

	writeJSON(w, http.StatusOK, map[string]string{"message": "user deleted"})
}

// Stats handles GET /api/v1/admin/stats
func (h *AdminHandler) Stats(w http.ResponseWriter, r *http.Request) {
	userCount, _ := h.userRepo.Count(r.Context())
	users, _ := h.userRepo.List(r.Context())

	var totalUsed, totalQuota int64
	for _, u := range users {
		totalUsed += u.UsedBytes
		totalQuota += u.QuotaBytes
	}

	writeJSON(w, http.StatusOK, map[string]interface{}{
		"total_users": userCount,
		"total_used":  totalUsed,
		"total_quota": totalQuota,
		"users":       users,
	})
}

// CreateInvite handles POST /api/v1/admin/invites
func (h *AdminHandler) CreateInvite(w http.ResponseWriter, r *http.Request) {
	userID := middleware.GetUserID(r.Context())

	var req struct {
		MaxUses    int    `json:"max_uses"`
		Role       string `json:"role"`
		QuotaBytes int64  `json:"quota_bytes"`
		Email      string `json:"email"`
		Message    string `json:"message"`
		SMTPServer string `json:"smtp_server"`
		SMTPPort   int    `json:"smtp_port"`
		SMTPUser   string `json:"smtp_user"`
		SMTPPass   string `json:"smtp_pass"`
		FromAddr   string `json:"from_address"`
		FromName   string `json:"from_name"`
		TLS        bool   `json:"tls"`
	}
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		writeError(w, "invalid request body", http.StatusBadRequest)
		return
	}

	if req.Role == "" {
		req.Role = "user"
	}
	if req.MaxUses == 0 {
		req.MaxUses = 1
	}
	if req.QuotaBytes <= 0 {
		req.QuotaBytes = 10737418240 // 10 GB default
	}

	// Generate random invite code
	code := generateRandomString(8)
	invite := &domain.InviteLink{
		Code:       code,
		CreatedBy:  userID,
		Role:       domain.Role(req.Role),
		QuotaBytes: req.QuotaBytes,
		MaxUses:    req.MaxUses,
	}

	if err := h.userRepo.CreateInvite(r.Context(), invite); err != nil {
		writeError(w, "failed to create invite", http.StatusInternalServerError)
		return
	}

	recipient := strings.TrimSpace(req.Email)
	adminSettingsMu.RLock()
	emailCfg, _ := adminSettings["email"].(map[string]interface{})
	generalCfg, _ := adminSettings["general"].(map[string]interface{})
	smtpServer := asString(emailCfg["smtp_server"])
	smtpPort := asInt(emailCfg["smtp_port"], 0)
	smtpUser := asString(emailCfg["smtp_user"])
	smtpPass := asString(emailCfg["smtp_pass"])
	fromAddress := asString(emailCfg["from_address"])
	fromName := asString(emailCfg["from_name"])
	useTLS := asBool(emailCfg["tls"], false)
	siteURL := strings.TrimSpace(asString(generalCfg["site_url"]))
	adminSettingsMu.RUnlock()

	if strings.TrimSpace(req.SMTPServer) != "" {
		smtpServer = strings.TrimSpace(req.SMTPServer)
	}
	if req.SMTPPort > 0 {
		smtpPort = req.SMTPPort
	}
	if req.SMTPUser != "" || req.SMTPPass != "" {
		smtpUser = req.SMTPUser
		smtpPass = req.SMTPPass
	}
	if strings.TrimSpace(req.FromAddr) != "" {
		fromAddress = strings.TrimSpace(req.FromAddr)
	}
	if strings.TrimSpace(req.FromName) != "" {
		fromName = strings.TrimSpace(req.FromName)
	}
	useTLS = req.TLS

	siteURL = siteBaseURL(siteURL, r)
	inviteURL := fmt.Sprintf("%s?invite=%s", siteURL, url.QueryEscape(invite.Code))

	emailSent := false
	emailError := ""
	if recipient != "" {
		if smtpServer == "" || smtpPort == 0 || fromAddress == "" {
			emailError = "smtp settings are incomplete: set server, port and from address in admin settings"
		} else {
			displayName := chooseDisplayName("", recipient)
			subject := "You're invited to FreeDrive"
			message := strings.TrimSpace(req.Message)

			body := fmt.Sprintf(
				"Hello %s,\n\nYou've been invited to join FreeDrive.\n\nInvite link:\n%s\n\nRole: %s\nQuota: %.1f GB\n",
				displayName,
				inviteURL,
				strings.ToUpper(string(invite.Role)),
				float64(invite.QuotaBytes)/(1024*1024*1024),
			)
			if message != "" {
				body += fmt.Sprintf("\nMessage from admin:\n%s\n", message)
			}
			body += "\nIf the link does not open automatically, copy and paste it into your browser.\n"

			if err := sendSMTPEmail(smtpServer, smtpPort, smtpUser, smtpPass, fromAddress, fromName, recipient, subject, body, useTLS); err != nil {
				emailError = "failed to send invite email: " + err.Error()
			} else {
				emailSent = true
			}
		}
	}

	writeJSON(w, http.StatusCreated, map[string]interface{}{
		"id":          invite.ID,
		"code":        invite.Code,
		"created_by":  invite.CreatedBy,
		"role":        invite.Role,
		"quota_bytes": invite.QuotaBytes,
		"max_uses":    invite.MaxUses,
		"used_count":  invite.UsedCount,
		"expires_at":  invite.ExpiresAt,
		"created_at":  invite.CreatedAt,
		"invite_url":  inviteURL,
		"email_sent":  emailSent,
		"email_error": emailError,
	})
}

// ResendInvite handles POST /api/v1/admin/invites/resend
func (h *AdminHandler) ResendInvite(w http.ResponseWriter, r *http.Request) {
	var req struct {
		Email      string `json:"email"`
		Code       string `json:"code"`
		Role       string `json:"role"`
		QuotaBytes int64  `json:"quota_bytes"`
		Message    string `json:"message"`
		SMTPServer string `json:"smtp_server"`
		SMTPPort   int    `json:"smtp_port"`
		SMTPUser   string `json:"smtp_user"`
		SMTPPass   string `json:"smtp_pass"`
		FromAddr   string `json:"from_address"`
		FromName   string `json:"from_name"`
		TLS        bool   `json:"tls"`
	}
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		writeError(w, "invalid request body", http.StatusBadRequest)
		return
	}

	recipient := strings.TrimSpace(req.Email)
	code := strings.TrimSpace(req.Code)
	if recipient == "" || code == "" {
		writeError(w, "email and invite code are required", http.StatusBadRequest)
		return
	}

	adminSettingsMu.RLock()
	emailCfg, _ := adminSettings["email"].(map[string]interface{})
	generalCfg, _ := adminSettings["general"].(map[string]interface{})
	smtpServer := asString(emailCfg["smtp_server"])
	smtpPort := asInt(emailCfg["smtp_port"], 0)
	smtpUser := asString(emailCfg["smtp_user"])
	smtpPass := asString(emailCfg["smtp_pass"])
	fromAddress := asString(emailCfg["from_address"])
	fromName := asString(emailCfg["from_name"])
	useTLS := asBool(emailCfg["tls"], false)
	siteURL := strings.TrimSpace(asString(generalCfg["site_url"]))
	adminSettingsMu.RUnlock()

	if strings.TrimSpace(req.SMTPServer) != "" {
		smtpServer = strings.TrimSpace(req.SMTPServer)
	}
	if req.SMTPPort > 0 {
		smtpPort = req.SMTPPort
	}
	if req.SMTPUser != "" || req.SMTPPass != "" {
		smtpUser = req.SMTPUser
		smtpPass = req.SMTPPass
	}
	if strings.TrimSpace(req.FromAddr) != "" {
		fromAddress = strings.TrimSpace(req.FromAddr)
	}
	if strings.TrimSpace(req.FromName) != "" {
		fromName = strings.TrimSpace(req.FromName)
	}
	useTLS = req.TLS

	if smtpServer == "" || smtpPort == 0 || fromAddress == "" {
		writeError(w, "smtp settings are incomplete: set server, port and from address in admin settings", http.StatusBadRequest)
		return
	}

	siteURL = siteBaseURL(siteURL, r)

	inviteURL := fmt.Sprintf("%s?invite=%s", siteURL, url.QueryEscape(code))
	role := strings.TrimSpace(req.Role)
	if role == "" {
		role = "user"
	}
	quota := req.QuotaBytes
	if quota <= 0 {
		quota = 10737418240
	}
	subject := "FreeDrive Invite Link (Resent)"
	body := fmt.Sprintf(
		"Hello %s,\n\nYour FreeDrive invite link has been resent.\n\nInvite link:\n%s\n\nRole: %s\nQuota: %.1f GB\n",
		chooseDisplayName("", recipient),
		inviteURL,
		strings.ToUpper(role),
		float64(quota)/(1024*1024*1024),
	)
	if strings.TrimSpace(req.Message) != "" {
		body += fmt.Sprintf("\nMessage from admin:\n%s\n", strings.TrimSpace(req.Message))
	}
	body += "\nIf the link does not open automatically, copy and paste it into your browser.\n"

	if err := sendSMTPEmail(smtpServer, smtpPort, smtpUser, smtpPass, fromAddress, fromName, recipient, subject, body, useTLS); err != nil {
		writeError(w, "failed to resend invite email: "+err.Error(), http.StatusInternalServerError)
		return
	}

	writeJSON(w, http.StatusOK, map[string]string{"status": "success", "message": "Invite email resent"})
}

// ListInvites handles GET /api/v1/admin/invites
func (h *AdminHandler) ListInvites(w http.ResponseWriter, r *http.Request) {
	invites, err := h.userRepo.ListInvites(r.Context())
	if err != nil {
		writeError(w, "failed to list invites", http.StatusInternalServerError)
		return
	}
	writeJSON(w, http.StatusOK, map[string]interface{}{"invites": invites})
}

// Activity handles GET /api/v1/admin/activity
func (h *AdminHandler) Activity(w http.ResponseWriter, r *http.Request) {
	page, _ := strconv.Atoi(r.URL.Query().Get("page"))
	pageSize, _ := strconv.Atoi(r.URL.Query().Get("page_size"))

	logs, total, err := h.activityRepo.ListAll(r.Context(), page, pageSize)
	if err != nil {
		writeError(w, "failed to list activity", http.StatusInternalServerError)
		return
	}

	writeJSON(w, http.StatusOK, map[string]interface{}{
		"activities": logs,
		"total":      total,
	})
}

// MyActivity handles GET /api/v1/activity (current authenticated user only).
func (h *AdminHandler) MyActivity(w http.ResponseWriter, r *http.Request) {
	userID := middleware.GetUserID(r.Context())
	if strings.TrimSpace(userID) == "" {
		writeError(w, "unauthorized", http.StatusUnauthorized)
		return
	}

	page, _ := strconv.Atoi(r.URL.Query().Get("page"))
	pageSize, _ := strconv.Atoi(r.URL.Query().Get("page_size"))

	logs, total, err := h.activityRepo.List(r.Context(), userID, page, pageSize)
	if err != nil {
		writeError(w, "failed to list activity", http.StatusInternalServerError)
		return
	}

	writeJSON(w, http.StatusOK, map[string]interface{}{
		"activities": logs,
		"total":      total,
	})
}

// GetSettings handles GET /api/v1/admin/settings
func (h *AdminHandler) GetSettings(w http.ResponseWriter, r *http.Request) {
	adminSettingsMu.RLock()
	defer adminSettingsMu.RUnlock()
	writeJSON(w, http.StatusOK, adminSettings)
}

// SaveSettings handles POST /api/v1/admin/settings
func (h *AdminHandler) SaveSettings(w http.ResponseWriter, r *http.Request) {
	var payload map[string]interface{}
	if err := json.NewDecoder(r.Body).Decode(&payload); err != nil {
		writeError(w, "invalid request body", http.StatusBadRequest)
		return
	}
	adminSettingsMu.Lock()
	for k, v := range payload {
		adminSettings[k] = v
	}
	adminSettingsMu.Unlock()
	saveSettings()
	writeJSON(w, http.StatusOK, map[string]string{"status": "ok"})
}

// RunBackupNow handles POST /api/v1/admin/backup/run
func (h *AdminHandler) RunBackupNow(w http.ResponseWriter, r *http.Request) {
	adminSettingsMu.RLock()
	backupCfg, _ := adminSettings["backup"].(map[string]interface{})
	location := strings.TrimSpace(asString(backupCfg["location"]))
	if location == "" {
		location = "/var/lib/freedrive/backups"
	}
	snapshot := map[string]interface{}{}
	for k, v := range adminSettings {
		snapshot[k] = v
	}
	adminSettingsMu.RUnlock()

	if err := os.MkdirAll(location, 0755); err != nil {
		writeError(w, "failed to create backup directory: "+err.Error(), http.StatusInternalServerError)
		return
	}

	now := time.Now()
	fileName := fmt.Sprintf("freedrive-backup-%s.json", now.Format("20060102-150405"))
	fullPath := filepath.Join(location, fileName)
	payload := map[string]interface{}{
		"created_at": now.UTC().Format(time.RFC3339),
		"kind":       "settings_snapshot",
		"settings":   snapshot,
	}
	bytes, err := json.MarshalIndent(payload, "", "  ")
	if err != nil {
		writeError(w, "failed to create backup payload", http.StatusInternalServerError)
		return
	}
	if err := os.WriteFile(fullPath, bytes, fs.FileMode(0644)); err != nil {
		writeError(w, "failed to write backup file: "+err.Error(), http.StatusInternalServerError)
		return
	}

	writeJSON(w, http.StatusOK, map[string]interface{}{
		"status":   "success",
		"at":       now.UTC().Format(time.RFC3339),
		"size":     formatBytes(int64(len(bytes))),
		"filename": fileName,
		"path":     fullPath,
	})
}

// SendPasswordReset handles POST /api/v1/admin/users/{id}/reset-password
func (h *AdminHandler) SendPasswordReset(w http.ResponseWriter, r *http.Request) {
	userID := chi.URLParam(r, "id")
	user, err := h.userRepo.GetByID(r.Context(), userID)
	if err != nil || user == nil {
		writeError(w, "user not found", http.StatusNotFound)
		return
	}
	if strings.TrimSpace(user.Email) == "" {
		writeError(w, "target user has no email address", http.StatusBadRequest)
		return
	}

	adminSettingsMu.RLock()
	emailCfg, _ := adminSettings["email"].(map[string]interface{})
	generalCfg, _ := adminSettings["general"].(map[string]interface{})
	smtpServer := asString(emailCfg["smtp_server"])
	smtpPort := asInt(emailCfg["smtp_port"], 0)
	smtpUser := asString(emailCfg["smtp_user"])
	smtpPass := asString(emailCfg["smtp_pass"])
	fromAddress := asString(emailCfg["from_address"])
	fromName := asString(emailCfg["from_name"])
	useTLS := asBool(emailCfg["tls"], false)
	siteURL := strings.TrimSpace(asString(generalCfg["site_url"]))
	adminSettingsMu.RUnlock()

	if smtpServer == "" || smtpPort == 0 || fromAddress == "" {
		writeError(w, "smtp settings are incomplete: set server, port and from address in admin settings", http.StatusBadRequest)
		return
	}

	siteURL = siteBaseURL(siteURL, r)

	token := createPasswordResetToken(user.Email)
	resetURL := fmt.Sprintf("%s/reset-password?token=%s&email=%s", siteURL, token, url.QueryEscape(user.Email))
	subject := "FreeDrive Password Reset"
	body := fmt.Sprintf(
		"Hello %s,\n\nA password reset was requested for your FreeDrive account.\n\nReset link:\n%s\n\nIf you did not request this, you can ignore this email.\n",
		chooseDisplayName(user.Username, user.Email),
		resetURL,
	)

	if err := sendSMTPEmail(smtpServer, smtpPort, smtpUser, smtpPass, fromAddress, fromName, user.Email, subject, body, useTLS); err != nil {
		writeError(w, "failed to send reset email: "+err.Error(), http.StatusInternalServerError)
		return
	}

	writeJSON(w, http.StatusOK, map[string]string{"status": "success", "message": "Password reset email sent"})
}

// TestEmail handles POST /api/v1/admin/test-email
func (h *AdminHandler) TestEmail(w http.ResponseWriter, r *http.Request) {
	var req struct {
		ToAddress   string `json:"to_address"`
		SMTPServer  string `json:"smtp_server"`
		SMTPPort    int    `json:"smtp_port"`
		SMTPUser    string `json:"smtp_user"`
		SMTPPass    string `json:"smtp_pass"`
		FromAddress string `json:"from_address"`
		FromName    string `json:"from_name"`
		TLS         bool   `json:"tls"`
	}

	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		writeError(w, "invalid request body", http.StatusBadRequest)
		return
	}

	if req.ToAddress == "" || req.SMTPServer == "" || req.SMTPPort == 0 || req.FromAddress == "" {
		writeError(w, "missing required email fields", http.StatusBadRequest)
		return
	}

	if err := sendSMTPEmail(
		req.SMTPServer,
		req.SMTPPort,
		req.SMTPUser,
		req.SMTPPass,
		req.FromAddress,
		req.FromName,
		req.ToAddress,
		"FreeDrive Test Email",
		"This is a test email sent from your FreeDrive Admin Panel.\nIf you received this, your SMTP configuration is correct!\n",
		req.TLS,
	); err != nil {
		writeError(w, "failed to send email: "+err.Error(), http.StatusInternalServerError)
		return
	}

	writeJSON(w, http.StatusOK, map[string]string{"status": "success", "message": "Email sent successfully"})
}

func sendSMTPEmail(smtpServer string, smtpPort int, smtpUser, smtpPass, fromAddress, fromName, toAddress, subject, body string, useTLS bool) error {
	if smtpPort == 443 || strings.HasPrefix(smtpServer, "https://") || strings.HasPrefix(smtpServer, "http://") || strings.Contains(smtpServer, "api.mailersend.com") || strings.Contains(smtpServer, "api.zeptomail.") {
		return sendHTTPEmail(smtpServer, smtpPass, fromAddress, fromName, toAddress, subject, body)
	}

	fromHeader := fromAddress
	if strings.TrimSpace(fromName) != "" {
		fromHeader = fmt.Sprintf("%s <%s>", fromName, fromAddress)
	}

	msg := []byte(
		"Subject: " + subject + "\r\n" +
			"From: " + fromHeader + "\r\n" +
			"To: " + toAddress + "\r\n" +
			"MIME-Version: 1.0\r\n" +
			"Content-Type: text/plain; charset=\"utf-8\"\r\n" +
			"\r\n" +
			body,
	)

	addr := fmt.Sprintf("%s:%d", smtpServer, smtpPort)
	var auth smtp.Auth
	if smtpUser != "" || smtpPass != "" {
		auth = smtp.PlainAuth("", smtpUser, smtpPass, smtpServer)
	}

	// Implicit TLS (typically port 465)
	if useTLS && smtpPort == 465 {
		tlsconfig := &tls.Config{
			InsecureSkipVerify: true,
			ServerName:         smtpServer,
		}
		conn, errConn := tls.Dial("tcp", addr, tlsconfig)
		if errConn != nil {
			return fmt.Errorf("failed to connect via TLS: %w", errConn)
		}
		client, errClient := smtp.NewClient(conn, smtpServer)
		if errClient != nil {
			return fmt.Errorf("failed to create SMTP client: %w", errClient)
		}
		defer client.Close()

		if auth != nil {
			if err := client.Auth(auth); err != nil {
				return fmt.Errorf("smtp auth failed: %w", err)
			}
		}
		if err := client.Mail(fromAddress); err != nil {
			return fmt.Errorf("smtp mail failed: %w", err)
		}
		if err := client.Rcpt(toAddress); err != nil {
			return fmt.Errorf("smtp rcpt failed: %w", err)
		}
		writer, errWriter := client.Data()
		if errWriter != nil {
			return fmt.Errorf("smtp data failed: %w", errWriter)
		}
		if _, err := writer.Write(msg); err != nil {
			return fmt.Errorf("failed to write email body: %w", err)
		}
		if err := writer.Close(); err != nil {
			return fmt.Errorf("failed to close email body writer: %w", err)
		}
		_ = client.Quit()
		return nil
	}

	// Explicit SMTP flow (supports STARTTLS, commonly port 587).
	client, err := smtp.Dial(addr)
	if err != nil {
		return fmt.Errorf("failed to dial SMTP server: %w", err)
	}
	defer client.Close()

	if useTLS {
		if ok, _ := client.Extension("STARTTLS"); ok {
			tlsConfig := &tls.Config{
				InsecureSkipVerify: true,
				ServerName:         smtpServer,
			}
			if err := client.StartTLS(tlsConfig); err != nil {
				return fmt.Errorf("starttls failed: %w", err)
			}
		} else {
			return fmt.Errorf("smtp server does not support STARTTLS")
		}
	}

	if auth != nil {
		if ok, _ := client.Extension("AUTH"); ok {
			if err := client.Auth(auth); err != nil {
				return fmt.Errorf("smtp auth failed: %w", err)
			}
		} else if smtpUser != "" || smtpPass != "" {
			return fmt.Errorf("smtp auth is required but not supported by server")
		}
	}

	if err := client.Mail(fromAddress); err != nil {
		return fmt.Errorf("smtp mail failed: %w", err)
	}
	if err := client.Rcpt(toAddress); err != nil {
		return fmt.Errorf("smtp rcpt failed: %w", err)
	}
	writer, errWriter := client.Data()
	if errWriter != nil {
		return fmt.Errorf("smtp data failed: %w", errWriter)
	}
	if _, err := writer.Write(msg); err != nil {
		return fmt.Errorf("failed to write email body: %w", err)
	}
	if err := writer.Close(); err != nil {
		return fmt.Errorf("failed to close email body writer: %w", err)
	}
	_ = client.Quit()
	return nil
}

func asString(v interface{}) string {
	switch x := v.(type) {
	case string:
		return x
	default:
		return ""
	}
}

func siteBaseURL(raw string, r *http.Request) string {
	raw = strings.TrimSpace(raw)
	if raw != "" {
		if u, err := url.Parse(raw); err == nil && u.Scheme != "" && u.Host != "" {
			return strings.TrimRight(fmt.Sprintf("%s://%s", u.Scheme, u.Host), "/")
		}
	}

	scheme := "http"
	if r.TLS != nil {
		scheme = "https"
	}
	return strings.TrimRight(fmt.Sprintf("%s://%s", scheme, r.Host), "/")
}

func asInt(v interface{}, fallback int) int {
	switch x := v.(type) {
	case float64:
		return int(x)
	case int:
		return x
	case int64:
		return int(x)
	case string:
		i, err := strconv.Atoi(strings.TrimSpace(x))
		if err == nil {
			return i
		}
	}
	return fallback
}

func asBool(v interface{}, fallback bool) bool {
	switch x := v.(type) {
	case bool:
		return x
	case string:
		s := strings.ToLower(strings.TrimSpace(x))
		return s == "1" || s == "true" || s == "yes" || s == "on"
	default:
		return fallback
	}
}

func chooseDisplayName(username, email string) string {
	if strings.TrimSpace(username) != "" {
		return username
	}
	return email
}

func formatBytes(n int64) string {
	if n < 1024 {
		return fmt.Sprintf("%d B", n)
	}
	kb := float64(n) / 1024
	if kb < 1024 {
		return fmt.Sprintf("%.1f KB", kb)
	}
	mb := kb / 1024
	if mb < 1024 {
		return fmt.Sprintf("%.1f MB", mb)
	}
	gb := mb / 1024
	return fmt.Sprintf("%.2f GB", gb)
}

func sendHTTPEmail(apiUrl, apiToken, fromAddress, fromName, toAddress, subject, body string) error {
	if !strings.HasPrefix(apiUrl, "http") {
		apiUrl = "https://" + apiUrl
	}

	var payload []byte
	var err error

	// Auto-detect Provider
	isZepto := false
	if strings.Contains(apiUrl, "mailersend") {
		if !strings.Contains(apiUrl, "/v1/email") {
			apiUrl = strings.TrimRight(apiUrl, "/") + "/v1/email"
		}
		
		reqBody := map[string]interface{}{
			"from":    map[string]string{"email": fromAddress, "name": fromName},
			"to":      []map[string]string{{"email": toAddress}},
			"subject": subject,
			"text":    body,
		}
		payload, err = json.Marshal(reqBody)
	} else if strings.Contains(apiUrl, "zeptomail") {
		isZepto = true
		if !strings.Contains(apiUrl, "/v1.1/email") {
			apiUrl = strings.TrimRight(apiUrl, "/") + "/v1.1/email"
		}

		reqBody := map[string]interface{}{
			"from": map[string]string{"address": fromAddress, "name": fromName},
			"to": []map[string]interface{}{
				{
					"email_address": map[string]string{"address": toAddress, "name": toAddress},
				},
			},
			"subject":  subject,
			"textbody": body,
		}
		payload, err = json.Marshal(reqBody)
	} else {
		// Generic JSON payload
		reqBody := map[string]string{
			"from_email": fromAddress,
			"from_name":  fromName,
			"to_email":   toAddress,
			"subject":    subject,
			"body":       body,
		}
		payload, err = json.Marshal(reqBody)
	}

	if err != nil {
		return err
	}

	req, err := http.NewRequest("POST", apiUrl, bytes.NewBuffer(payload))
	if err != nil {
		return err
	}

	req.Header.Set("Content-Type", "application/json")
	req.Header.Set("Accept", "application/json")
	if apiToken != "" {
		if isZepto {
			if !strings.HasPrefix(apiToken, "Zoho-enczapikey ") {
				apiToken = "Zoho-enczapikey " + apiToken
			}
			req.Header.Set("Authorization", apiToken)
		} else {
			if !strings.HasPrefix(apiToken, "Bearer ") {
				apiToken = "Bearer " + apiToken
			}
			req.Header.Set("Authorization", apiToken)
		}
	}

	client := &http.Client{Timeout: 10 * time.Second}
	resp, err := client.Do(req)
	if err != nil {
		return fmt.Errorf("HTTP request failed: %w", err)
	}
	defer resp.Body.Close()

	if resp.StatusCode >= 400 {
		respBody, _ := io.ReadAll(resp.Body)
		return fmt.Errorf("HTTP %d: %s", resp.StatusCode, string(respBody))
	}

	return nil
}
